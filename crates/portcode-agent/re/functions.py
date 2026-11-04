# PortCode helper: list the functions of a binary with LIEF. Input: JSON in argv[1]. Output: JSON on stdout.
import json, os, sys

def _hook(t, e, tb):
    print(json.dumps({"error": "%s: %s" % (t.__name__, e)}), flush=True); os._exit(0)
sys.excepthook = _hook
import lief

a = json.loads(sys.argv[1])
b = lief.parse(a["file"])
if b is None:
    print(json.dumps({"error": "not an executable format LIEF understands (Mach-O, ELF, PE)"})); sys.exit(0)

fmt = str(b.format).split(".")[-1]
imagebase = getattr(b, "imagebase", 0) or 0
funcs = {}  # vaddr -> name

def add(addr, name):
    if addr and addr not in funcs:
        funcs[addr] = name or ""

if fmt == "MACHO":
    macho = b.concrete if hasattr(b, "concrete") else b
    if hasattr(b, "at"):  # fat binary: first slice
        macho = b.at(0)
    b = macho
    for s in b.symbols:
        if s.type == 0 or (s.value and s.origin and "SYMTAB" in str(s.origin)):
            pass
        if s.value and s.name and not s.name.startswith(("radr:", "mh_", "__mh_", "ltmp", "l_")):
            add(s.value, s.name.lstrip("_") if s.name.startswith("_") else s.name)
    fs = getattr(b, "function_starts", None)
    if fs:
        for f in fs.functions:
            add(f + imagebase if f < imagebase else f, "")
    entry = b.entrypoint
    add(entry, "entry")
else:
    for s in getattr(b, "functions", []):
        add(s.address, s.name)
    for s in getattr(b, "symbols", []):
        if "FUNC" in str(getattr(s, "type", "")) and s.value:
            add(s.value, s.name)
    for s in getattr(b, "exported_functions", []):
        add(s.address, s.name)
    add(b.entrypoint, "entry")

def offset_of(addr):
    try:
        return b.virtual_address_to_offset(addr)
    except Exception:
        return None

addrs = sorted(funcs)
out = []
for i, ad in enumerate(addrs):
    nxt = addrs[i + 1] if i + 1 < len(addrs) else None
    size = (nxt - ad) if nxt and nxt - ad < 0x10000 else 0
    off = offset_of(ad)
    if not isinstance(off, int) or off < 0:
        off = None
    out.append({"addr": ad, "name": funcs[ad] or "sub_%x" % ad, "size": size, "offset": off})
lim = int(a.get("limit", 2000))
arch = str(getattr(getattr(b, "header", None), "cpu_type", "") or getattr(getattr(b, "header", None), "machine_type", "") or "").split(".")[-1]
print(json.dumps({"format": fmt, "arch": arch, "entry": b.entrypoint, "functions": out[:lim], "total": len(out)}))
sys.stdout.flush()
os._exit(0)  # skip LIEF's noisy leak report at interpreter shutdown
