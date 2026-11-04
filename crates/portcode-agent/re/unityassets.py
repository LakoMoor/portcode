# PortCode helper: read Unity asset files and bundles with UnityPy. Input: JSON in argv[1]. Output: JSON on stdout.
import json, os, sys

def _out(o):
    print(json.dumps(o, default=str), flush=True); os._exit(0)
def _hook(t, e, tb):
    _out({"error": "%s: %s" % (t.__name__, e)})
sys.excepthook = _hook
import UnityPy

a = json.loads(sys.argv[1])
mode, q, lim = a.get("mode", "list"), (a.get("query") or "").lower(), int(a.get("limit", 200))
path = a["file"]
try:
    env = UnityPy.load(path)
except Exception as e:
    _out({"error": "UnityPy could not open this (%s). Give a .assets, .unity3d, .bundle, level*, sharedassets* or globalgamemanagers file." % e})
objs = list(env.objects)
if not objs:
    _out({"error": "no Unity objects found: not a Unity asset file or bundle, or a Unity version UnityPy does not read. Try globalgamemanagers, level0, sharedassets0.assets or a .bundle."})

def name_of(o):
    try:
        d = o.read()
        return getattr(d, "m_Name", None) or getattr(d, "name", "") or ""
    except Exception:
        return ""

if mode == "info":
    counts = {}
    for o in objs:
        counts[o.type.name] = counts.get(o.type.name, 0) + 1
    _out({"unity_version": str(getattr(env.objects[0].assets_file, "unity_version", "")) if objs else "", "objects": len(objs),
          "classes": dict(sorted(counts.items(), key=lambda kv: -kv[1])[:40])})

if mode == "list":
    cls = (a.get("class") or "").lower()
    out = []
    for o in objs:
        if cls and o.type.name.lower() != cls:
            continue
        n = name_of(o)
        if q and q not in n.lower():
            continue
        out.append({"class": o.type.name, "path_id": o.path_id, "name": n, "size": o.byte_size})
        if len(out) >= lim:
            break
    _out({"objects": out, "total": len(objs)})

if mode in ("text", "tree"):
    pid = a.get("path_id")
    hit = [o for o in objs if (pid is not None and o.path_id == int(pid)) or (pid is None and q and name_of(o).lower() == q)]
    if not hit:
        _out({"error": "no object with that path_id or exact name. Use mode=list first."})
    o = hit[0]
    if mode == "text":
        if o.type.name != "TextAsset":
            _out({"error": "object is a %s, not a TextAsset; use mode=tree" % o.type.name})
        d = o.read()
        t = getattr(d, "m_Script", "")
        if isinstance(t, bytes):
            t = t.decode("utf-8", "replace")
        _out({"name": name_of(o), "text": t[: int(a.get("max", 20000))]})
    try:
        tree = o.read_typetree()
    except Exception as e:
        _out({"error": "no type tree for this object (%s). The game may strip type trees; a matching assembly is needed." % e})
    _out({"class": o.type.name, "name": name_of(o), "tree": tree})

_out({"error": "mode must be info, list, text or tree"})
