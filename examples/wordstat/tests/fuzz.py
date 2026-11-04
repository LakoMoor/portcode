#!/usr/bin/env python3
"""Differential fuzz: random files + args through the C original and the Rust port; any byte-level difference fails.
usage: fuzz.py <original-bin> <port-bin> [iterations] [seed]   (run with python3 -I)"""
import os, random, subprocess, sys, tempfile
orig, port = sys.argv[1], sys.argv[2]
n = int(sys.argv[3]) if len(sys.argv) > 3 else 400
random.seed(int(sys.argv[4]) if len(sys.argv) > 4 else 1234)
alphabet = list("abcXYZ019 \n\t.,;'-_") + ["é", "\x00", "\xff"]
bad = 0
for _ in range(n):
    k = random.choice([0, 1, 5, 50, 500, 3000])
    data = "".join(random.choice(alphabet) for _ in range(k)).encode("latin-1", "replace")
    if random.random() < 0.1: data += b"w" * random.randint(250, 300)
    with tempfile.NamedTemporaryFile(delete=False) as f: f.write(data); p = f.name
    args = []
    if random.random() < .6: args += ["--top", random.choice(["0", "1", "3", "10", "-2", "abc", "99999999999", "7x"])]
    if random.random() < .5: args += ["--min-len", random.choice(["0", "1", "2", "5", "300", "-1", "z"])]
    args += [p]
    if random.random() < .05: args = args[:-1] + ["--top"]
    r = [subprocess.run([b] + args, capture_output=True) for b in (orig, port)]
    if [(x.stdout, x.stderr, x.returncode) for x in r[:1]] != [(r[1].stdout, r[1].stderr, r[1].returncode)]:
        bad += 1; print("MISMATCH", args)
        if bad > 3: break
    os.unlink(p)
print(f"differential fuzz: {n - bad}/{n} identical" if not bad else f"{bad} mismatches")
sys.exit(1 if bad else 0)
