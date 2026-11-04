#!/usr/bin/env bash
# rustport parity — compare golden outputs of the ORIGINAL against the PORT, with numeric tolerance.
# Usage: parity.sh <golden_dir> <port_dir> [abs_tol=1e-5] [rel_tol=1e-4]
# Files are matched by relative path. Text/JSON/CSV are compared token-wise (numbers within tolerance);
# images (png) are compared per-channel max diff (needs python3 + Pillow, else byte compare); others byte-for-byte.
set -u
g="${1:?golden_dir}"; p="${2:?port_dir}"; at="${3:-1e-5}"; rt="${4:-1e-4}"
exec python3 -I - "$g" "$p" "$at" "$rt" <<'PY'
import sys, os, re, math
g, p, at, rt = sys.argv[1], sys.argv[2], float(sys.argv[3]), float(sys.argv[4])
num = re.compile(r'-?\d+\.?\d*(?:[eE][-+]?\d+)?|nan|inf', re.I)
def close(a, b): return a == b or abs(a-b) <= at + rt*max(abs(a), abs(b))
def cmp_text(a, b):
    ta, tb = re.split(r'(\s+|[,;:=()\[\]{}"])', a), re.split(r'(\s+|[,;:=()\[\]{}"])', b)
    if len(ta) != len(tb): return f"token count {len(ta)} vs {len(tb)}"
    for i, (x, y) in enumerate(zip(ta, tb)):
        if x == y: continue
        if num.fullmatch(x) and num.fullmatch(y) and close(float(x), float(y)): continue
        return f"token {i}: {x!r} vs {y!r}"
def cmp_png(fa, fb):
    try: from PIL import Image, ImageChops
    except ImportError: return None
    A, B = Image.open(fa).convert("RGBA"), Image.open(fb).convert("RGBA")
    if A.size != B.size: return f"size {A.size} vs {B.size}"
    m = max(e[1] for e in ImageChops.difference(A, B).getextrema())
    return None if m <= 2 else f"max channel diff {m}"
if not os.path.isdir(g) or not os.path.isdir(p):
    print('ERROR: both golden and port directories must exist')
    sys.exit(2)
bad = missing = ok = 0
for root, _, files in os.walk(g):
    for f in files:
        rel = os.path.relpath(os.path.join(root, f), g)
        fa, fb = os.path.join(g, rel), os.path.join(p, rel)
        if not os.path.exists(fb): print(f"MISSING {rel}"); missing += 1; continue
        err = None
        if f.lower().endswith(".png"):
            err = cmp_png(fa, fb)
            if err is None and open(fa,'rb').read() != open(fb,'rb').read():
                try: import PIL
                except ImportError: err = "bytes differ (install Pillow for tolerant compare)"
        else:
            try: err = cmp_text(open(fa, encoding="utf-8").read(), open(fb, encoding="utf-8").read())
            except UnicodeDecodeError: err = None if open(fa,'rb').read() == open(fb,'rb').read() else "binary differs"
        if err: print(f"DIFF    {rel}: {err}"); bad += 1
        else: ok += 1
if not (ok or bad or missing):
    print('EMPTY golden directory: no tests were executed')
    bad += 1
for root, _, files in os.walk(p):
    for f in files:
        rel = os.path.relpath(os.path.join(root, f), p)
        if not os.path.isfile(os.path.join(g, rel)):
            print(f'EXTRA   {rel}: unexpected port output')
            bad += 1
print(f"parity: ok={ok} diff={bad} missing={missing}")
sys.exit(1 if bad or missing else 0)
PY
