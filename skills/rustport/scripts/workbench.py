#!/usr/bin/env python3
"""rustport workbench: static inventory, versioned evidence ledger, strict parity checks.
Python standard library only. Does NOT execute reference games or claim independent oracle capture.
"""
from __future__ import annotations
import argparse
import datetime as dt
import fnmatch
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import platform
import sqlite3
import subprocess
import sys

STATES = ('unknown', 'observed', 'traced', 'implemented', 'compared', 'verified', 'blocked')
CONFIDENCES = ('unknown', 'inferred', 'observed', 'measured')
MAX_DIFFERENCES = 50


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec='seconds')


def dump(x):
    print(json.dumps(x, ensure_ascii=False, indent=2, sort_keys=True))


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def strict_json(source):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('duplicate JSON key: ' + key)
            result[key] = value
        return result
    def reject(value):
        raise ValueError('non-finite JSON number: ' + value)
    return json.loads(source, object_pairs_hook=pairs, parse_constant=reject)


def valid_trace(value):
    if not isinstance(value, (list, dict)) or not value:
        raise ValueError('empty JSON/JSONL trace cannot establish parity')
    if isinstance(value, list) and not all(isinstance(x, dict) and x for x in value):
        raise ValueError('JSONL trace must contain nonempty event/state objects')


def read_json(path):
    p = Path(path)
    if p.suffix.lower() == '.jsonl':
        with p.open(encoding='utf-8') as f:
            return [strict_json(line) for line in f if line.strip()]
    return strict_json(p.read_text(encoding='utf-8'))


def write_json(path, data):
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + '\n', encoding='utf-8')


def cmd_doctor(args):
    commands = {'required': ['python3'], 'optional': ['cargo', 'rustc', 'rustfmt', 'git', 'file', 'strings', 'nm', 'objdump', 'llvm-objdump', 'analyzeHeadless', 'ghidraRun', 'ghidra', 'lldb', 'gdb', 'java', 'dotnet', 'wine', 'ffmpeg']}
    results = {k: {tool: shutil.which(tool) for tool in items} for k, items in commands.items()}
    results['python'] = {'version': sys.version.split()[0], 'ok': sys.version_info >= (3, 9)}
    dump(results)
    return 0 if results['python']['ok'] else 1


def put_new(path, content):
    p = Path(path)
    if p.exists():
        return False
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(content, encoding='utf-8')
    return True


def cmd_init(args):
    root = Path(args.project).expanduser().resolve()
    root.mkdir(parents=True, exist_ok=True)
    made = []
    files = {
        'docs/SCOPE.md': f'# {args.name}: port scope\n\nReference game version: {args.source_version}\nTarget platform: TODO\nCompatibility goal: TODO\nOriginal source is owned/accessed locally: TODO\nOriginal runs for testing: TODO\nNo original binaries, game assets, or proprietary decompiled code in this repository.\n',
        'docs/FINDINGS.md': '# Findings\n\nRecord evidence here. Label each claim observed / measured / inferred / unknown.\n',
        'docs/TRACES.md': '# Original trace evidence\n\nRecord build hash, address/path, input, output, and observation command.\n',
        'docs/CONTRACTS.md': '# API, data and behavior contracts\n\nRecord state machines, file formats, numeric rules and coordinate systems.\n',
        'docs/PORT_PLAN.md': '# Port plan\n\n- [ ] Inventory the owned original\n- [ ] Observe one deterministic mechanic\n- [ ] Build original oracle and matching Rust probe\n- [ ] Verify one vertical slice\n- [ ] Integrate renderer only after core contract is stable\n',
        '.gitignore': '# Local originals, extracted content, private evidence, generated outputs\n.rustport/\nreference-game/\noriginal-assets/\noriginal-decompiled/\nproprietary/\nport-ledger.sqlite\nport-ledger.sqlite-*\nreference.lock.json\ntarget/\n*.pdb\n*.dSYM/\n',
        'AGENTS.md': '# Project agent instructions\n\nUse the rustport skill. Read docs/SCOPE.md and ledger before coding.\nDo not commit original game files or reverse-engineering dumps. Preserve independent reference probes.\nOne behavior per patch; run cargo test and workbench parity when an oracle exists.\nNever claim verified parity without a passing comparison of independently collected reference and port traces.\n',
        'CLAUDE.md': '# Claude Code project instructions\n\nFollow `AGENTS.md`, use the `rustport` skill, and read `docs/SCOPE.md` before porting.\nNever publish original game content. Only mark parity verified with evidence-backed original-vs-Rust tests.\n',
        'evidence/README.md': '# Evidence directory\n\nPut non-proprietary traces only. Do not publish original game assets or private logs.\nOriginal trace and port trace must come from independent implementations.\n',
        'evidence/original/.gitkeep': '',
        'evidence/port/.gitkeep': '',
    }
    for path, body in files.items():
        if put_new(root / path, body):
            made.append(path)
    db = root / 'port-ledger.sqlite'
    connect_db(db).close()
    result = {'project': str(root), 'created': made, 'existing_files_preserved': True, 'ledger': str(db)}
    dump(result)
    return 0

MAGICS = [(b'MZ', 'PE/DOS executable (MZ header)'), (b'\x7fELF', 'ELF executable/library'),
          (b'PK\x03\x04', 'ZIP container'), (b'\x89PNG\r\n\x1a\n', 'PNG'),
          (b'OggS', 'Ogg'), (b'RIFF', 'RIFF container'), (b'\x1f\x8b', 'gzip'),
          (b'\xca\xfe\xba\xbe', 'Java class or Mach-O fat; inspect further'),
          (b'\xfe\xed\xfa\xcf', 'Mach-O x86_64/64-bit big-endian variant'),
          (b'\xcf\xfa\xed\xfe', 'Mach-O 64-bit little-endian variant'),
          (b'\xfe\xed\xfa\xce', 'Mach-O 32-bit big-endian variant'),
          (b'\xce\xfa\xed\xfe', 'Mach-O 32-bit little-endian variant'),
          (b'\xca\xfe\xba\xbe', 'fat Mach-O / Java class')]


def classify(path):
    try:
        with path.open('rb') as f:
            data = f.read(16)
    except (OSError, PermissionError):
        return 'unreadable'
    if data[:4] == b'\xca\xfe\xba\xbe':
        return 'Java class / fat Mach-O (ambiguous)'
    for sig, title in MAGICS:
        if data.startswith(sig):
            return title
    return path.suffix.lower() or '(no extension)'


def cmd_inventory(args):
    root = Path(args.source).expanduser().resolve()
    if not root.is_dir():
        raise ValueError('source must be an existing directory')
    if args.limit < 1 or args.hash_limit_mb < 0:
        raise ValueError('inventory limits must be nonnegative, limit must be positive')
    dest = Path(args.output).expanduser().resolve()
    if dest == root or root in dest.parents:
        raise ValueError('output must be OUTSIDE the original game directory')
    entries, by_kind = [], {}
    truncated = False
    for base, dirs, files in os.walk(root, followlinks=False):
        dirs[:] = sorted(d for d in dirs if not (Path(base) / d).is_symlink())
        for filename in sorted(files):
            path = Path(base) / filename
            if path.is_symlink() or not path.is_file():
                continue
            if len(entries) >= args.limit:
                truncated = True
                break
            try:
                size = path.stat().st_size
            except OSError:
                continue
            kind = classify(path)
            by_kind[kind] = by_kind.get(kind, 0) + 1
            record = {'path': path.relative_to(root).as_posix(), 'bytes': size, 'kind': kind}
            if args.hash_all or size <= args.hash_limit_mb * 1024 * 1024:
                try:
                    record['sha256'] = sha256(path)
                except OSError:
                    record['sha256'] = None
            else:
                record['hash_skipped_large'] = True
            entries.append(record)
        if truncated:
            break
    manifest = {'schema': 1, 'generated_at': now(), 'source_label': root.name, 'total_listed': len(entries),
                'truncated': truncated, 'limit': args.limit, 'kind_counts': by_kind, 'files': entries,
                'note': 'Static inventory only; no file was executed or copied. Hashes skipped where flagged.'}
    write_json(dest, manifest)
    dump({'output': str(dest), 'total_listed': len(entries), 'truncated': truncated, 'kinds': by_kind})
    return 0

def run_static_command(executable, arguments, timeout=10):
    if not shutil.which(executable):
        return {'available': False}
    try:
        r = subprocess.run([executable, *arguments], stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           text=True, errors='replace', timeout=timeout, check=False)
        return {'available': True, 'returncode': r.returncode,
                'stdout_excerpt': r.stdout[:8192], 'stderr_excerpt': r.stderr[:2000],
                'truncated': len(r.stdout) > 8192}
    except subprocess.TimeoutExpired:
        return {'available': True, 'timed_out': True}


def cmd_inspect(args):
    path = Path(args.binary).expanduser().resolve()
    if not path.is_file(): raise ValueError('binary must be an existing file')
    results = {'path_label': path.name, 'size': path.stat().st_size, 'sha256': sha256(path),
               'magic_or_extension': classify(path),
               'warning': 'Tools statically inspect a potentially hostile file. Isolate untrusted inputs; never execute them.',
               'probes': {}}
    results['probes']['file'] = run_static_command('file', [str(path)])
    if shutil.which('readelf'):
        results['probes']['readelf_header'] = run_static_command('readelf', ['-h', str(path)])
        results['probes']['readelf_notes'] = run_static_command('readelf', ['-n', str(path)])
    elif shutil.which('llvm-readelf'):
        results['probes']['llvm-readelf_header'] = run_static_command('llvm-readelf', ['-h', str(path)])
    if shutil.which('otool'):
        results['probes']['otool_header'] = run_static_command('otool', ['-hv', str(path)])
    if shutil.which('llvm-nm'):
        results['probes']['symbols_excerpt'] = run_static_command('llvm-nm', ['-g', str(path)])
    elif shutil.which('nm'):
        results['probes']['symbols_excerpt'] = run_static_command('nm', ['-g', str(path)])
    if args.output: write_json(args.output, results)
    dump(results)
    return 0


SCHEMA = '''CREATE TABLE IF NOT EXISTS ports (
 id TEXT PRIMARY KEY NOT NULL,
 subsystem TEXT NOT NULL DEFAULT 'misc',
 origin TEXT NOT NULL DEFAULT '',
 target TEXT NOT NULL DEFAULT '',
 status TEXT NOT NULL DEFAULT 'unknown',
 confidence TEXT NOT NULL DEFAULT 'unknown',
 evidence TEXT NOT NULL DEFAULT '',
 notes TEXT NOT NULL DEFAULT '',
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 verified_at TEXT NOT NULL DEFAULT '',
 oracle_file TEXT NOT NULL DEFAULT '',
 port_file TEXT NOT NULL DEFAULT '',
 oracle_sha256 TEXT NOT NULL DEFAULT '',
 port_sha256 TEXT NOT NULL DEFAULT '',
 policy TEXT NOT NULL DEFAULT '',
 wired INTEGER NOT NULL DEFAULT 0,
 reference_sha256 TEXT NOT NULL DEFAULT '',
 reference_lock_sha256 TEXT NOT NULL DEFAULT '',
 reference_lock_file TEXT NOT NULL DEFAULT ''
);'''


def connect_db(path):
    path = Path(path).expanduser().resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(str(path))
    con.row_factory = sqlite3.Row
    con.execute(SCHEMA)
    con.commit()
    return con


def rowdict(row):
    return dict(row) if row is not None else None


def cmd_ledger(args):
    db = Path(args.db).expanduser().resolve()
    con = connect_db(db)
    try:
        if args.ledger_op == 'add':
            con.execute('INSERT INTO ports(id,subsystem,origin,target,status,confidence,evidence,notes,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)',
                (args.id, args.subsystem, args.origin, args.target, 'unknown', args.confidence, args.evidence, args.notes, now(), now()))
            con.commit()
            dump({'added': args.id})
        elif args.ledger_op == 'set':
            if args.status in ('verified', 'compared'):
                raise ValueError('Compared/verified requires passing parity; run parity --db ... --id ...')
            row = con.execute('SELECT * FROM ports WHERE id=?', (args.id,)).fetchone()
            if not row:
                raise ValueError('Unknown ledger ID: ' + args.id)
            updates = {'updated_at': now()}
            for field in ('status', 'target', 'origin', 'evidence', 'notes', 'confidence', 'subsystem', 'wired'):
                val = getattr(args, field)
                if val is not None:
                    updates[field] = val
            # Editing a verified record invalidates its verification proof.
            if row['status'] in ('verified', 'compared'):
                updates.update(status=args.status or 'implemented', verified_at='', oracle_sha256='', port_sha256='', policy='', reference_sha256='', reference_lock_sha256='', reference_lock_file='')
            pairs = ', '.join(f'{key}=?' for key in updates)
            con.execute(f'UPDATE ports SET {pairs} WHERE id=?', (*updates.values(), args.id))
            con.commit()
            dump({'updated': args.id, 'values': updates})
        elif args.ledger_op == 'show':
            row = con.execute('SELECT * FROM ports WHERE id=?', (args.id,)).fetchone()
            if not row: raise ValueError('Unknown ledger ID: ' + args.id)
            dump(rowdict(row))
        elif args.ledger_op == 'list':
            q = 'SELECT id,subsystem,origin,target,status,confidence,evidence FROM ports'
            params = ()
            if args.status:
                q += ' WHERE status=?'; params = (args.status,)
            q += ' ORDER BY subsystem,id'
            dump([rowdict(r) for r in con.execute(q, params).fetchall()])
        elif args.ledger_op == 'stats':
            counts = {s: 0 for s in STATES}
            for r in con.execute('SELECT status, count(*) c FROM ports GROUP BY status'):
                counts[r['status']] = r['c']
            dump({'total': sum(counts.values()), 'statuses': counts, 'verified_fraction':
                  (counts['verified']/sum(counts.values()) if sum(counts.values()) else 0)})
        elif args.ledger_op == 'audit':
            stale = []
            for r in con.execute("SELECT * FROM ports WHERE status IN ('verified','compared')"):
                reasons = []
                for kind in ('oracle', 'port'):
                    path = stored_path_to_abs(db.parent, r[f'{kind}_file'])
                    if not path.is_file(): reasons.append(f'{kind} file missing')
                    elif sha256(path) != r[f'{kind}_sha256']: reasons.append(f'{kind} file changed')
                if r['status'] == 'verified':
                    lockfile = stored_path_to_abs(db.parent, r['reference_lock_file'])
                    if not lockfile.is_file():
                        reasons.append('reference lock missing')
                    elif sha256(lockfile) != r['reference_lock_sha256']:
                        reasons.append('reference lock changed')
                    else:
                        try:
                            lock = read_json(lockfile)
                            exe = Path(lock['original_path'])
                            if not exe.is_file() or sha256(exe) != r['reference_sha256']:
                                reasons.append('original executable changed or missing')
                        except (KeyError, OSError, ValueError):
                            reasons.append('invalid reference lock')
                if not r['target'] or not r['evidence'] or not r['origin']:
                    reasons.append('missing origin/target/evidence')
                if reasons: stale.append({'id': r['id'], 'reasons': reasons})
            dump({'compared_or_verified_checked': con.execute("SELECT count(*) FROM ports WHERE status IN ('verified','compared')").fetchone()[0],
                  'stale': stale, 'pass': not stale})
            return 0 if not stale else 1
        return 0
    finally:
        con.close()


def stored_path_to_abs(dbparent, name):
    p = Path(name)
    return p if p.is_absolute() else dbparent / p


def path_for_db(dbparent, path):
    p = Path(path).expanduser().resolve()
    try:
        return p.relative_to(dbparent).as_posix()
    except ValueError:
        return str(p)


def should_ignore(pointer, ignored):
    return any(fnmatch.fnmatchcase(pointer, pattern) for pattern in ignored)


def compare(a, b, *, abs_tol, rel_tol, ignore, pointer='', differences=None):
    if differences is None: differences = []
    if len(differences) >= MAX_DIFFERENCES: return differences
    if should_ignore(pointer, ignore): return differences
    if isinstance(a, bool) or isinstance(b, bool):
        equal = type(a) is type(b) and a == b
    elif isinstance(a, (int,float)) and isinstance(b, (int,float)):
        equal = math.isclose(a, b, abs_tol=abs_tol, rel_tol=rel_tol) if abs_tol or rel_tol else a == b
    else:
        equal = type(a) is type(b) and a == b if not isinstance(a,(dict,list)) else type(a) is type(b)
    if not equal:
        differences.append({'path': pointer or '/', 'expected': repr(a)[:200], 'actual': repr(b)[:200]})
        return differences
    if isinstance(a, dict):
        for key in sorted(set(a.keys()) | set(b.keys())):
            child = pointer + '/' + str(key).replace('~','~0').replace('/','~1')
            if should_ignore(child, ignore): continue
            if key not in a or key not in b:
                differences.append({'path':child, 'expected':'<missing>' if key not in a else repr(a[key])[:200],
                                    'actual':'<missing>' if key not in b else repr(b[key])[:200]})
            else:
                compare(a[key],b[key],abs_tol=abs_tol,rel_tol=rel_tol,ignore=ignore,pointer=child,differences=differences)
            if len(differences)>=MAX_DIFFERENCES: break
    elif isinstance(a,list):
        if len(a)!=len(b):
            differences.append({'path':pointer or '/', 'expected_length':len(a),'actual_length':len(b)})
        for i in range(min(len(a),len(b))):
            if len(differences)>=MAX_DIFFERENCES: break
            compare(a[i],b[i],abs_tol=abs_tol,rel_tol=rel_tol,ignore=ignore,pointer=pointer+'/'+str(i),differences=differences)
    return differences


def cmd_parity(args):
    reference = Path(args.original).expanduser().resolve()
    port = Path(args.port).expanduser().resolve()
    if reference == port:
        raise ValueError('original and port paths must be different (independent runners required)')
    if args.reference_lock and not (args.db and args.id):
        raise ValueError('--reference-lock requires --db and --id')
    if args.abs_tol < 0 or args.rel_tol < 0:
        raise ValueError('tolerances must be nonnegative')
    if not math.isfinite(args.abs_tol) or not math.isfinite(args.rel_tol):
        raise ValueError('tolerances must be finite')
    if reference.suffix.lower() not in ('.json', '.jsonl'):
        raise ValueError('only JSON and JSONL are supported')
    if reference.suffix.lower() != port.suffix.lower():
        raise ValueError('trace formats must match (.json or .jsonl)')
    a = read_json(reference)
    b = read_json(port)
    valid_trace(a)
    valid_trace(b)
    differences = compare(a,b,abs_tol=args.abs_tol,rel_tol=args.rel_tol,ignore=args.ignore)
    result = {'pass': not differences, 'status': 'PASS' if not differences else 'FAIL', 'original':str(reference),
              'port':str(port), 'original_sha256':sha256(reference), 'port_sha256':sha256(port),
              'abs_tol':args.abs_tol,'rel_tol':args.rel_tol,'ignored_json_pointers':args.ignore,
              'differences':differences,'differences_truncated':len(differences)>=MAX_DIFFERENCES,
              'checked_at':now(), 'warning':'User/agent must establish independent original-game oracle provenance.'}
    if (args.db and not args.id) or (args.id and not args.db):
        raise ValueError('Both --db and --id are required together')
    if result['pass'] and args.db and args.id:
        db = Path(args.db).expanduser().resolve()
        con = connect_db(db)
        try:
            row = con.execute('SELECT * FROM ports WHERE id=?', (args.id,)).fetchone()
            if not row:
                raise ValueError('Unknown ledger ID: '+args.id)
            if not row['origin'] or not row['target'] or not row['evidence']:
                raise ValueError('Ledger row must document origin, target and evidence before verified')
            status, ref_hash, lock_hash, lock_file = 'compared', '', '', ''
            if args.reference_lock:
                lock_path = Path(args.reference_lock).expanduser().resolve()
                lock = read_json(lock_path)
                if lock.get('schema') != 1 or 'original_path' not in lock or 'sha256' not in lock:
                    raise ValueError('reference lock has invalid schema')
                original = Path(lock['original_path'])
                if not original.is_file() or sha256(original) != lock['sha256']:
                    raise ValueError('reference binary changed since pin')
                if not row['wired']:
                    raise ValueError('mark runtime wiring in ledger before verification')
                status, ref_hash, lock_hash, lock_file = 'verified', lock['sha256'], sha256(lock_path), path_for_db(db.parent, lock_path)
            con.execute('UPDATE ports SET status=?, verified_at=?, updated_at=?, oracle_file=?, port_file=?, oracle_sha256=?, port_sha256=?, policy=?, reference_sha256=?, reference_lock_sha256=?, reference_lock_file=? WHERE id=?',
                        (status,now(),now(),path_for_db(db.parent,reference),path_for_db(db.parent,port),
                         result['original_sha256'],result['port_sha256'],json.dumps({'abs_tol':args.abs_tol,'rel_tol':args.rel_tol,'ignore':args.ignore}),ref_hash,lock_hash,lock_file,args.id))
            con.commit()
            result['ledger_status'] = status
            result['ledger_id'] = args.id
        finally: con.close()
    if args.report: write_json(args.report, result)
    dump(result)
    return 0 if not differences else 1


def parser():
    ap=argparse.ArgumentParser(prog='rustport workbench',description=__doc__)
    subs=ap.add_subparsers(dest='command',required=True)
    subs.add_parser('doctor',help='Report installed tools without modifying system').set_defaults(func=cmd_doctor)
    ini=subs.add_parser('init',help='Create evidence docs and SQLite ledger, without replacing app/workspace files')
    ini.add_argument('--project',required=True); ini.add_argument('--name',required=True); ini.add_argument('--source-version',default='unknown')
    ini.set_defaults(func=cmd_init)
    inv=subs.add_parser('inventory',help='Static inventory and optional SHA256, does not run files')
    inv.add_argument('--source',required=True); inv.add_argument('--output',required=True)
    inv.add_argument('--limit',type=int,default=10000); inv.add_argument('--hash-limit-mb',type=int,default=16); inv.add_argument('--hash-all',action='store_true')
    inv.set_defaults(func=cmd_inventory)
    inspect=subs.add_parser('inspect',help='Static fingerprint for one native binary; never executes it')
    inspect.add_argument('--binary',required=True); inspect.add_argument('--output'); inspect.set_defaults(func=cmd_inspect)
    ledger=subs.add_parser('ledger',help='Manage per-function/per-behavior evidence ledger')
    ledger.add_argument('--db',default='port-ledger.sqlite')
    lo=ledger.add_subparsers(dest='ledger_op',required=True)
    add=lo.add_parser('add'); add.add_argument('--id',required=True); add.add_argument('--subsystem',default='misc'); add.add_argument('--origin',default=''); add.add_argument('--target',default=''); add.add_argument('--evidence',default=''); add.add_argument('--notes',default=''); add.add_argument('--confidence',choices=CONFIDENCES,default='unknown'); add.set_defaults(func=cmd_ledger)
    edit=lo.add_parser('set'); edit.add_argument('--id',required=True); edit.add_argument('--wired',type=int, choices=(0,1)); edit.add_argument('--status',choices=('unknown','observed','traced','implemented','blocked')); edit.add_argument('--target'); edit.add_argument('--origin'); edit.add_argument('--evidence'); edit.add_argument('--notes'); edit.add_argument('--confidence',choices=CONFIDENCES); edit.add_argument('--subsystem'); edit.set_defaults(func=cmd_ledger)
    show=lo.add_parser('show'); show.add_argument('--id',required=True); show.set_defaults(func=cmd_ledger)
    li=lo.add_parser('list'); li.add_argument('--status',choices=STATES); li.set_defaults(func=cmd_ledger)
    lo.add_parser('stats').set_defaults(func=cmd_ledger)
    lo.add_parser('audit').set_defaults(func=cmd_ledger)
    par=subs.add_parser('parity',help='Strict diff of separately captured JSON/JSONL oracle and port traces')
    par.add_argument('original'); par.add_argument('port'); par.add_argument('--abs-tol',type=float,default=0); par.add_argument('--rel-tol',type=float,default=0)
    par.add_argument('--ignore',action='append',default=[],help='Explicit JSON Pointer glob, e.g. /0/timestamp')
    par.add_argument('--report'); par.add_argument('--db'); par.add_argument('--id'); par.add_argument('--reference-lock'); par.set_defaults(func=cmd_parity)
    pin=subs.add_parser('pin',help='Hash and pin an original owned executable without running it')
    pin.add_argument('--original',required=True); pin.add_argument('--output',required=True); pin.add_argument('--version',default='unknown')
    pin.set_defaults(func=cmd_pin)
    return ap


def cmd_pin(args):
    exe = Path(args.original).expanduser().resolve(strict=True)
    lock_path = Path(args.output).expanduser().resolve()
    if not exe.is_file() or exe == lock_path:
        raise ValueError('Original must be an existing file different from lock output')
    lock = {'schema': 1, 'original_path': str(exe), 'sha256': sha256(exe), 'version': args.version,
            'platform': platform.platform(), 'captured_at': now(),
            'note': 'Pins file identity only: does not prove runtime parity or establish trace independence.'}
    if lock_path.exists():
        raise ValueError('Refusing to overwrite an existing reference lock')
    write_json(lock_path, lock)
    dump({'reference_lock': str(lock_path), 'sha256': lock['sha256']})
    return 0


def main():
    args=parser().parse_args()
    try:
        return args.func(args)
    except (ValueError,OSError,sqlite3.Error,json.JSONDecodeError) as exc:
        print('ERROR:', str(exc),file=sys.stderr)
        return 2

if __name__ == '__main__':
    raise SystemExit(main())
