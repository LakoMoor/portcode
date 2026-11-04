"""Local byte-exact oracle checks. Goldens were captured before Rust existed."""
from pathlib import Path
import hashlib, json, random, subprocess, tempfile, time
ROOT = Path(__file__).resolve().parents[1]
G = json.loads((ROOT / 'tests/golden/original.json').read_text())
assert hashlib.sha256((ROOT / 'original/main.c').read_bytes()).hexdigest() == G['source_sha256']
C = ROOT / 'tests/original'
R = ROOT / 'port/target/debug/rustport-cli'
subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', str(ROOT/'original/main.c'), '-o', str(C)], check=True)
def run(binary, commands, directory):
    r = subprocess.run([str(binary)], input=commands.encode(), cwd=directory, capture_output=True, timeout=10)
    saved = Path(directory, 'saved.txt')
    return dict(stdout=r.stdout.decode(), stderr=r.stderr.decode(), code=r.returncode, saved=saved.read_text() if saved.exists() else None)
for name, case in G['cases'].items():
    for binary in (C, R, ROOT / "port/target/release/rustport-cli"):
        if not binary.exists(): continue
        with tempfile.TemporaryDirectory() as d:
            for f, s in G['fixtures'].items(): Path(d, f).write_text(s)
            result = run(binary, case['input'], d)
            assert result == {k:v for k,v in case.items() if k != 'input'}, (name, binary, result)
print('13 frozen golden cases pass for C and Rust (stdout, stderr, status, save bytes)')
# Cross-language saves in both directions, including a second save after continuing.
for writer, reader in ((C,R), (R,C)):
    with tempfile.TemporaryDirectory() as d:
        first = run(writer, 'right tick tick left tick save saved.txt', d)
        state = Path(d,'saved.txt').read_text()
        expected = run(writer, 'load saved.txt tick stop tick save continued.txt', d)
        expected_save = Path(d,'continued.txt').read_bytes()
        actual = run(reader, 'load saved.txt tick stop tick save continued.txt', d)
        assert actual == expected
        assert Path(d,'continued.txt').read_bytes() == expected_save
        assert Path(d,'saved.txt').read_text() == state
print('2 cross-language save/load continuations pass')
rng = random.Random(20261029)
for i in range(120):
    commands = ' '.join(rng.choices(['left','right','stop','tick','state'], weights=[1,1,1,8,1], k=300))
    with tempfile.TemporaryDirectory() as d:
        assert run(C,commands,d) == run(R,commands,d), i
# fscanf whitespace, signs, adjacent signed fields, malformed/truncated fields.
for contents in ['', '1', '1 2', '1 2 3', 'x 2 3 4', '1 2 3 +', '1 2 3 4tail', '1-2+3-4', '\t-8\n+9 0 4294967295', '2147483647 -2147483648 0 0', '0 0 0 4294967296']:
    with tempfile.TemporaryDirectory() as d:
        Path(d,'input').write_text(contents)
        assert run(C,'state load input state',d) == run(R,'state load input state',d), contents
for commands in ['state\x00suffix tick', 'state'+' '*40+'tick', 'a'*32, 'save '+ 'a'*256]:
    with tempfile.TemporaryDirectory() as d:
        assert run(C,commands,d) == run(R,commands,d), repr(commands)
print('120 seeded differential traces, 11 save parser cases, 4 token cases pass')
# Performance smoke measurement includes startup and output capture, not a benchmark.
release = ROOT/'port/target/release/rustport-cli'
if release.exists():
    commands = 'tick '*100000
    times=[]
    for binary in (C,release):
        with tempfile.TemporaryDirectory() as d:
            start=time.perf_counter(); result=run(binary,commands,d); times.append(time.perf_counter()-start)
            assert result['code']==0
    print('100000 ticks including process/output: C %.4fs Rust release %.4fs' % tuple(times))
