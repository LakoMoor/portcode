# PortCode helper: read .NET (C#, Unity Mono) assembly metadata with dnfile. Input: JSON in argv[1]. Output: JSON on stdout.
import json, os, sys

def _out(o):
    print(json.dumps(o), flush=True); os._exit(0)
def _hook(t, e, tb):
    _out({"error": "%s: %s" % (t.__name__, e)})
sys.excepthook = _hook
import logging
import dnfile
logging.getLogger('dnfile').setLevel(logging.CRITICAL)
logging.disable(logging.CRITICAL)

a = json.loads(sys.argv[1])
mode, q, lim = a.get("mode", "info"), (a.get("query") or "").lower(), int(a.get("limit", 300))
try:
    pe = dnfile.dnPE(a["file"])
except Exception as e:
    _out({"error": "not a .NET assembly (%s). Native code needs disasm/decompile instead." % e})
net = pe.net
if net is None or net.mdtables is None:
    _out({"error": "this PE file has no .NET metadata (it is native code). Use disasm/decompile, or `unity` to check for IL2CPP."})
t = net.mdtables

def s(x):
    return x.value.decode("utf-8", "replace") if hasattr(x, "value") and isinstance(x.value, bytes) else (str(x.value) if hasattr(x, "value") else str(x))
def rows(tab):
    return list(tab.rows) if tab is not None and hasattr(tab, "rows") else []

types = rows(t.TypeDef)
methods = rows(t.MethodDef)
fields = rows(t.Field)

def full(td):
    ns = s(td.TypeNamespace) if td.TypeNamespace else ""
    return (ns + "." if ns else "") + s(td.TypeName)

def span(i, tab_rows, listname, nxt_td, count):
    """Rows of a list table (MethodList/FieldList) owned by TypeDef i; dnfile resolves them as row indexes."""
    lst = getattr(types[i], listname, None) or []
    return [r.row if hasattr(r, "row") else r for r in lst]

if mode == "info":
    asm = rows(t.Assembly)
    name = s(asm[0].Name) if asm else "?"
    ver = "%s.%s.%s.%s" % (asm[0].MajorVersion, asm[0].MinorVersion, asm[0].BuildNumber, asm[0].RevisionNumber) if asm else "?"
    rt = getattr(net.metadata, "Version", b"")
    rt = rt.decode() if isinstance(rt, bytes) else str(rt)
    out = {"assembly": name, "version": ver, "runtime": rt.strip("\x00"), "types": len(types), "methods": len(methods), "fields": len(fields),
           "refs": [s(r.Name) for r in rows(t.AssemblyRef)][:60]}
    _out(out)

if mode == "types":
    out = []
    for i, td in enumerate(types):
        n = full(td)
        if n == "<Module>" or (q and q not in n.lower()):
            continue
        base = ""
        ext = getattr(td, "Extends", None)
        try:
            base = full(ext.row) if ext is not None and hasattr(ext, "row") and hasattr(ext.row, "TypeName") else (s(ext.row.Name) if ext is not None and hasattr(ext, "row") else "")
        except Exception:
            base = ""
        out.append({"type": n, "base": base, "methods": len(getattr(td, "MethodList", []) or []), "fields": len(getattr(td, "FieldList", []) or [])})
        if len(out) >= lim:
            break
    _out({"types": out, "total": len(types)})

if mode == "type":
    if not q:
        _out({"error": "mode=type needs query (a type name or part of it)."})
    hit = [td for td in types if q in full(td).lower()]
    if not hit:
        _out({"error": "no type matches %r. Call mode=types first." % q})
    td = hit[0]
    ms = []
    for m in (getattr(td, "MethodList", []) or []):
        r = m.row if hasattr(m, "row") else m
        fl = r.Flags
        ms.append({"name": s(r.Name), "static": bool(getattr(fl, "mdStatic", False)), "public": bool(getattr(fl, "mdPublic", False)), "virtual": bool(getattr(fl, "mdVirtual", False)),
                   "rva": getattr(r, "Rva", 0), "params": [s(p.row.Name) for p in (getattr(r, "ParamList", []) or []) if hasattr(p, "row") and getattr(p.row, "Name", None)]})
    fs = []
    for f in (getattr(td, "FieldList", []) or []):
        r = f.row if hasattr(f, "row") else f
        fs.append({"name": s(r.Name), "static": bool(getattr(r.Flags, "fdStatic", False))})
    _out({"type": full(td), "matches": [full(x) for x in hit[:10]], "methods": ms[:lim], "fields": fs[:lim]})

if mode == "strings":
    us = net.user_strings
    out = []
    if us is not None:
        try:
            size = us.sizeof() if hasattr(us, "sizeof") else 0
            i = 1
            while i < size and len(out) < lim * 4:
                v = us.get(i)
                if v is None:
                    i += 1; continue
                txt = v.value if hasattr(v, "value") else str(v)
                txt = txt.decode("utf-16-le", "replace") if isinstance(txt, bytes) else str(txt)
                if txt and (not q or q in txt.lower()):
                    out.append(txt)
                i += max(1, len(v.raw_data) if hasattr(v, "raw_data") and v.raw_data else 2)
        except Exception as e:
            _out({"error": "could not read user strings: %s" % e})
    _out({"strings": out[:lim], "total": len(out)})

if mode == "refs":
    pinv = []
    for r in rows(t.ImplMap):
        try:
            pinv.append("%s!%s" % (s(r.ImportScope.row.Name), s(r.ImportName)))
        except Exception:
            pass
    _out({"assembly_refs": ["%s %s.%s" % (s(r.Name), r.MajorVersion, r.MinorVersion) for r in rows(t.AssemblyRef)], "pinvoke": pinv[:lim]})

_out({"error": "mode must be info, types, type, strings or refs"})
