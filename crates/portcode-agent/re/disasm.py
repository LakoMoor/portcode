# PortCode helper: disassemble bytes with Capstone. Input: JSON in argv[1]. Output: JSON on stdout.
import json, sys
import capstone as cs

ARCH = {
    "x86": (cs.CS_ARCH_X86, cs.CS_MODE_32), "x64": (cs.CS_ARCH_X86, cs.CS_MODE_64),
    "arm": (cs.CS_ARCH_ARM, cs.CS_MODE_ARM), "thumb": (cs.CS_ARCH_ARM, cs.CS_MODE_THUMB),
    "arm64": (cs.CS_ARCH_ARM64, cs.CS_MODE_ARM),
    "mips": (cs.CS_ARCH_MIPS, cs.CS_MODE_MIPS32 | cs.CS_MODE_BIG_ENDIAN),
    "mipsel": (cs.CS_ARCH_MIPS, cs.CS_MODE_MIPS32 | cs.CS_MODE_LITTLE_ENDIAN),
    "riscv64": (cs.CS_ARCH_RISCV, cs.CS_MODE_RISCV64), "riscv32": (cs.CS_ARCH_RISCV, cs.CS_MODE_RISCV32),
    "ppc64": (cs.CS_ARCH_PPC, cs.CS_MODE_64 | cs.CS_MODE_BIG_ENDIAN),
}
a = json.loads(sys.argv[1])

def _hook(t, e, tb):  # a Python traceback is useless to the model: return one JSON error line instead
    print(json.dumps({"error": "%s: %s" % (t.__name__, e)})); sys.exit(0)
sys.excepthook = _hook
try:
    arch, mode = ARCH[a["arch"]]
except KeyError:
    print(json.dumps({"error": "unknown arch %r; use one of %s" % (a.get("arch"), ", ".join(sorted(ARCH)))})); sys.exit(0)
if a.get("hex"):
    data = bytes.fromhex(a["hex"].replace(" ", ""))
else:
    with open(a["file"], "rb") as f:
        f.seek(int(a.get("offset", 0)))
        data = f.read(int(a.get("length", 256)))
md = cs.Cs(arch, mode)
base = int(a.get("base", 0))
out, n = [], int(a.get("count", 200))
for i in md.disasm(data, base):
    out.append("%x: %-24s %s %s" % (i.address, i.bytes.hex(), i.mnemonic, i.op_str))
    if len(out) >= n:
        break
print(json.dumps({"lines": out, "bytes": len(data)}))
