#!/usr/bin/env python3
"""docs/demo/session.js from the JSON lines `portcode run --json` printed for the scripted wordstat session.
Usage: build_demo.py events.jsonl "<prompt>"   (the events come from real tool runs; only the model is scripted)"""
import json, sys, pathlib

src, prompt = sys.argv[1], sys.argv[2]
steps, text, pending = [{"k": "user", "text": prompt}], "", None

def flush():
    global text
    if text.strip():
        steps.append({"k": "say", "text": text.strip()})
    text = ""

def short(args):
    try:
        a = json.loads(args)
    except Exception:
        return args[:90]
    for k in ("path", "command", "golden", "subject"):
        if k in a:
            return str(a[k])[:90]
    return ""

for line in open(src):
    line = line.strip()
    if not line.startswith("{"):
        continue
    e = json.loads(line)
    t = e.get("type")
    if t == "text":
        text += e["text"]
    elif t == "tool_call":
        flush()
        pending = {"k": "tool", "name": e["name"], "arg": short(e["args"])}
    elif t == "tool_result" and pending:
        out = [l for l in e["text"].strip().splitlines() if l.strip()][:5]
        pending.update({"title": e.get("title", ""), "ok": bool(e.get("ok")), "out": out})
        steps.append(pending)
        pending = None
    elif t == "todos":
        steps.append({"k": "todos", "items": [[i["content"], i["status"] == "completed"] for i in e["items"]]})
flush()
out = pathlib.Path(__file__).resolve().parent.parent / "docs/demo/session.js"
out.write_text("window.PC_DEMO=" + json.dumps(steps, ensure_ascii=False, separators=(",", ":")) + ";\n")
print("wrote", out, len(steps), "steps")
