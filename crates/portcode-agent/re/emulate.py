# PortCode helper: run a code snippet in Unicorn with a hard instruction budget. Input: JSON in argv[1].
import json, sys, importlib
import unicorn as uc

ARCH = {  # name -> (uc arch, uc mode, const module, default sp reg, capstone arch/mode)
    "x86": ("UC_ARCH_X86", "UC_MODE_32", "x86_const", "ESP"),
    "x64": ("UC_ARCH_X86", "UC_MODE_64", "x86_const", "RSP"),
    "arm": ("UC_ARCH_ARM", "UC_MODE_ARM", "arm_const", "SP"),
    "thumb": ("UC_ARCH_ARM", "UC_MODE_THUMB", "arm_const", "SP"),
    "arm64": ("UC_ARCH_ARM64", "UC_MODE_ARM", "arm64_const", "SP"),
    "mips": ("UC_ARCH_MIPS", "UC_MODE_MIPS32 | uc.UC_MODE_BIG_ENDIAN", "mips_const", "SP"),
    "mipsel": ("UC_ARCH_MIPS", "UC_MODE_MIPS32 | uc.UC_MODE_LITTLE_ENDIAN", "mips_const", "SP"),
    "riscv64": ("UC_ARCH_RISCV", "UC_MODE_RISCV64", "riscv_const", "SP"),
}
a = json.loads(sys.argv[1])

def _hook(t, e, tb):  # a Python traceback is useless to the model: return one JSON error line instead
    print(json.dumps({"error": "%s: %s" % (t.__name__, e)})); sys.exit(0)
sys.excepthook = _hook

def fail(msg):
    print(json.dumps({"error": msg})); sys.exit(0)

if a["arch"] not in ARCH:
    fail("unknown arch %r; use one of %s" % (a["arch"], ", ".join(sorted(ARCH))))
arch_n, mode_n, cmod_n, sp = ARCH[a["arch"]]
const = importlib.import_module("unicorn." + cmod_n)
mode = eval("uc." + mode_n)
mu = uc.Uc(getattr(uc, arch_n), mode)
prefix = {"x86_const": "UC_X86_REG_", "arm_const": "UC_ARM_REG_", "arm64_const": "UC_ARM64_REG_",
          "mips_const": "UC_MIPS_REG_", "riscv_const": "UC_RISCV_REG_"}[cmod_n]

def reg(name):
    r = getattr(const, prefix + name.upper(), None)
    if r is None:
        fail("unknown register %r for %s" % (name, a["arch"]))
    return r

PAGE = 0x1000
def up(n): return (n + PAGE - 1) // PAGE * PAGE
if a.get("hex"):
    code = bytes.fromhex(a["hex"].replace(" ", ""))
else:
    with open(a["file"], "rb") as f:
        f.seek(int(a.get("offset", 0)))
        code = f.read(int(a.get("length", 0x10000)))
base = int(a.get("base", 0x100000))
mbase = base & ~(PAGE - 1)  # Unicorn maps whole pages; real load addresses are rarely page-aligned
mu.mem_map(mbase, up(len(code) + (base - mbase) + PAGE))
mu.mem_write(base, code)
stack = int(a.get("stack_base", 0x7fff0000)); ssize = 0x100000
mu.mem_map(stack, ssize)
mu.reg_write(reg(sp), stack + ssize - 0x1000)
for m in a.get("maps", []):  # extra zeroed memory: {"addr":..., "size":..., "hex": optional}
    mu.mem_map(int(m["addr"]), up(int(m["size"])))
    if m.get("hex"):
        mu.mem_write(int(m["addr"]), bytes.fromhex(m["hex"]))
for k, v in a.get("regs", {}).items():
    mu.reg_write(reg(k), int(v))

trace, limit = [], int(a.get("trace", 0))
if limit:
    import capstone as cs
    cmap = {"x86": (cs.CS_ARCH_X86, cs.CS_MODE_32), "x64": (cs.CS_ARCH_X86, cs.CS_MODE_64),
            "arm": (cs.CS_ARCH_ARM, cs.CS_MODE_ARM), "thumb": (cs.CS_ARCH_ARM, cs.CS_MODE_THUMB),
            "arm64": (cs.CS_ARCH_ARM64, cs.CS_MODE_ARM)}
    md = cs.Cs(*cmap[a["arch"]]) if a["arch"] in cmap else None
    def hook(u, addr, size, _):
        if len(trace) < limit:
            b = bytes(u.mem_read(addr, size))
            txt = ""
            if md:
                for i in md.disasm(b, addr):
                    txt = "%s %s" % (i.mnemonic, i.op_str)
            trace.append("%x: %s %s" % (addr, b.hex(), txt))
    mu.hook_add(uc.UC_HOOK_CODE, hook)

count = [0]
def counter(u, addr, size, _): count[0] += 1
mu.hook_add(uc.UC_HOOK_CODE, counter)
start = int(a.get("start", base)); end = int(a.get("end", base + len(code)))
err = None
try:
    mu.emu_start(start, end, count=int(a.get("max_insns", 100000)))
except uc.UcError as e:
    err = str(e)
names = a.get("show_regs") or [n[len(prefix):].lower() for n in dir(const)
                               if n.startswith(prefix) and n[len(prefix):].lower() in
                               ("rax rbx rcx rdx rsi rdi rbp rsp rip eax ebx ecx edx esi edi ebp esp eip "
                                "r0 r1 r2 r3 r4 r5 r6 r7 sp lr pc x0 x1 x2 x3 x4 x5 x6 x7 v0 v1 a0 a1 a2 a3 ra").split()]
regs = {}
for n in names:
    try: regs[n] = hex(mu.reg_read(reg(n)))
    except Exception: pass
dumps = []
for d in a.get("dump", []):
    try: dumps.append({"addr": hex(int(d["addr"])), "hex": bytes(mu.mem_read(int(d["addr"]), int(d["len"]))).hex()})
    except uc.UcError as e: dumps.append({"addr": hex(int(d["addr"])), "error": str(e)})
print(json.dumps({"instructions": count[0], "error": err, "regs": regs, "trace": trace, "dump": dumps}))
