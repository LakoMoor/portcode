#!/usr/bin/env python3
"""rustport native lab: bounded Ghidra headless, local function slices, Unicorn x86_64 probes.

Ghidra and Unicorn are OPTIONAL external tools. No executable is run by Ghidra.
Never commit the resulting Ghidra JSON or extracted original machine code.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
GHIDRA_SCRIPT = HERE / 'ghidra' / 'RustportExport.java'
MAX_FILE = 1024 * 1024


def digest(p):
    h = hashlib.sha256()
    with open(p, 'rb') as fp:
        for chunk in iter(lambda: fp.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def write_json(path, obj):
    p = Path(path).expanduser().resolve()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True) + '\n', encoding='utf-8')


def must_file(path):
    p = Path(path).expanduser().resolve(strict=True)
    if not p.is_file():
        raise ValueError('Expected a file: ' + str(p))
    return p


def headless_path(value=None):
    if value:
        path = Path(value).expanduser().resolve()
        if path.is_file():
            return path
        raise ValueError('Headless executable does not exist: ' + str(path))
    home = os.environ.get('GHIDRA_HOME')
    paths = []
    if home:
        paths += [Path(home)/'support'/'analyzeHeadless', Path(home)/'support'/'analyzeHeadless.bat']
    for name in ('analyzeHeadless', 'analyzeHeadless.bat'):
        exe = shutil.which(name)
        if exe:
            paths.append(Path(exe))
    return next((p.resolve() for p in paths if p.is_file()), None)


def doctor(_args):
    try:
        import unicorn
        ver = getattr(unicorn, '__version__', 'present')
    except ImportError:
        ver = None
    result = {'python': sys.version.split()[0], 'ghidra_headless': str(headless_path() or ''),
              'ghidra_script': str(GHIDRA_SCRIPT) if GHIDRA_SCRIPT.is_file() else None,
              'unicorn_version': ver,
              'note': 'Ghidra and Unicorn are optional; doctor does not execute binaries.'}
    print(json.dumps(result, indent=2)); return 0


def run_bounded(argv, timeout, cwd=None):
    # Isolate child process groups on POSIX; kill process group on timeout.
    kw = {'stdout': subprocess.PIPE, 'stderr': subprocess.STDOUT, 'text': True,
          'errors': 'replace', 'stdin': subprocess.DEVNULL, 'cwd': cwd}
    if os.name == 'posix': kw['start_new_session'] = True
    p = subprocess.Popen(argv, **kw)
    try:
        out, _ = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        if os.name == 'posix': os.killpg(p.pid, signal.SIGKILL)
        else: p.kill()
        out, _ = p.communicate()
        raise TimeoutError('Ghidra timed out after %d seconds; output tail: %s' % (timeout, out[-1600:]))
    return p.returncode, out


def analyze(args):
    binary = must_file(args.binary)
    exe = headless_path(args.headless)
    if not exe:
        raise ValueError('Ghidra headless is unavailable; pass --headless /path/to/analyzeHeadless or set GHIDRA_HOME')
    if args.max_functions < 1 or args.max_functions > 100000 or not 0 <= args.max_edges <= 10000 or not 0 <= args.decompile_limit <= 500:
        raise ValueError('Function/edge/decompilation limits are out of range')
    if args.decompile_limit > args.max_functions:
        raise ValueError('decompile-limit cannot exceed max-functions')
    if not 5 <= args.timeout <= 7200:
        raise ValueError('timeout must be between 5 and 7200 seconds')
    output = Path(args.output).expanduser().resolve()
    if output == binary or output.exists():
        raise ValueError('Refusing to overwrite input or an existing analysis result')
    if not GHIDRA_SCRIPT.is_file(): raise ValueError('Bundled Ghidra script missing')
    artifact_hash = digest(binary)
    with tempfile.TemporaryDirectory(prefix='rustport-ghidra-') as td:
        tmp = Path(td)
        (tmp/'projects').mkdir()
        temp_result = tmp/'analysis.json'
        command = [str(exe), str(tmp/'projects'), 'RustportScan', '-import', str(binary),
                   '-scriptPath', str(GHIDRA_SCRIPT.parent), '-postScript', GHIDRA_SCRIPT.name,
                   str(temp_result), str(args.max_functions), str(args.max_edges), str(args.decompile_limit),
                   '-log', str(tmp/'ghidra.log'), '-scriptlog', str(tmp/'script.log')]
        if args.dry_run:
            print(json.dumps({'command_preview': command, 'binary_sha256': artifact_hash, 'note': 'Preview only: ephemeral output paths will differ'}, indent=2))
            return 0
        if not args.allow_analysis:
            raise ValueError('Analysis requires --allow-analysis (Ghidra imports an untrusted file; use a VM for suspicious files)')
        code, logs = run_bounded(command, args.timeout)
        if code != 0 or not temp_result.is_file():
            raise RuntimeError('Ghidra headless failed or did not produce an export (exit=%s): %s' % (code, logs[-3000:]))
        data = json.loads(temp_result.read_text(encoding='utf-8'))
        if data.get('schema') != 1 or data.get('tool') != 'ghidra-headless' or not isinstance(data.get('functions'), list):
            raise ValueError('Ghidra export malformed; refusing to accept partial analysis')
        if not data['functions']:
            raise ValueError('Ghidra exported zero functions; cannot treat this as a successful recovery')
        data['source'] = {'filename': binary.name, 'bytes': binary.stat().st_size, 'sha256': artifact_hash}
        data['capture'] = {'decompilation_requested': args.decompile_limit,
                           'function_limit': args.max_functions, 'edge_limit_per_function': args.max_edges,
                           'evidence_kind': 'static-inference-not-runtime-observation'}
        write_json(output, data)
        print(json.dumps({'status': 'analyzed', 'output': str(output), 'functions': len(data['functions']),
                          'sha256': artifact_hash, 'truncated': data.get('functions_truncated')}, indent=2))
        return 0


def slice_bytes(args):
    binary = must_file(args.binary)
    output = Path(args.output).expanduser().resolve()
    if output.exists() or output == binary:
        raise ValueError('Output exists or aliases the input')
    if not 0 <= args.offset < binary.stat().st_size or not 1 <= args.length <= 65536:
        raise ValueError('Offset/length out of bounds; max slice 65536 bytes')
    if args.offset + args.length > binary.stat().st_size:
        raise ValueError('Slice would read beyond end of input')
    if not args.allow_slice:
        raise ValueError('Explicit --allow-slice required: extracted machine code must remain local')
    with binary.open('rb') as stream:
        stream.seek(args.offset)
        raw = stream.read(args.length)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('xb') as out: out.write(raw)
    manifest = {'schema': 1, 'origin_file': str(binary), 'origin_sha256': digest(binary),
                'file_offset': args.offset, 'length': len(raw), 'slice_sha256': digest(output),
                'caution': 'Raw FILE offset, not virtual address (VA). For PE/ELF map VA to file offset yourself.',
                'status': 'raw-extraction-only-no-runtime-oracle'}
    write_json(str(output)+'.provenance.json', manifest)
    print(json.dumps({'output': str(output), 'provenance': str(output)+'.provenance.json', 'sha256': manifest['slice_sha256']}, indent=2))
    return 0


def ingest(args):
    from workbench import connect_db, now
    source = must_file(args.export)
    data = json.loads(source.read_text(encoding='utf-8'))
    if data.get('schema') != 1 or data.get('tool') != 'ghidra-headless' or not isinstance(data.get('functions'),list):
        raise ValueError('Only compatible Ghidra exports are accepted')
    binary_hash = data.get('source',{}).get('sha256','')
    if not re.fullmatch('[0-9a-f]{64}', binary_hash):
        raise ValueError('Missing SHA-256 of original analyzed binary')
    if not data['functions']:
        raise ValueError('Cannot ingest empty function list')
    if not 1 <= args.limit <= 100000:
        raise ValueError('Invalid ingest limit')
    db = Path(args.db).expanduser().resolve()
    con = connect_db(db)
    added = 0; skipped = 0
    try:
        for item in data['functions'][:args.limit]:
            addr = item.get('entry')
            if not isinstance(addr,str) or not re.fullmatch(r'[0-9A-Fa-f]{1,16}',addr):
                raise ValueError('Invalid Ghidra entry address')
            row_id = f"native.{binary_hash[:12]}.{addr.lower()}"
            # Analyze ≠ observe: static extraction gets inferred confidence and unknown status.
            evidence = str(source)
            notes = f"Ghidra static; name={item.get('name','?')}; calls={len(item.get('calls',[]))}; NOT runtime verified"
            try:
                con.execute('INSERT INTO ports(id,subsystem,origin,target,status,confidence,evidence,notes,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)',
                            (row_id,args.subsystem,f"{binary_hash[:12]}:{addr}",'','unknown','inferred',evidence,notes,now(),now()))
                added += 1
            except sqlite3.IntegrityError:
                skipped += 1
        con.commit()
    finally: con.close()
    print(json.dumps({'added':added,'skipped_existing':skipped,'source_sha256':binary_hash,
                      'note':'No function is marked implemented or verified'},indent=2)); return 0



def load_ghidra_export(path):
    data = json.loads(must_file(path).read_text(encoding='utf-8'))
    if data.get('schema') != 1 or data.get('tool') != 'ghidra-headless':
        raise ValueError('Expected a rustport Ghidra export')
    if not isinstance(data.get('functions'),list) or not data['functions']:
        raise ValueError('Empty or malformed Ghidra function list')
    return data


def rank_functions(args):
    data = load_ghidra_export(args.export)
    if not 1 <= args.limit <= 10000: raise ValueError('Invalid rank limit')
    lookup = {f['entry'].lower():f for f in data['functions'] if isinstance(f,dict) and isinstance(f.get('entry'),str)}
    incoming = {key:0 for key in lookup}
    for f in lookup.values():
        for edge in f.get('calls',[]):
            to = str(edge.get('to','')).lower()
            if to in incoming: incoming[to] += 1
    ranked=[]
    for addr,fn in lookup.items():
        # A prioritization heuristic only: higher fan-in and fan-out imply centrality,
        # but do not prove gameplay relevance or runtime reachability.
        out=len(fn.get('calls',[]))
        score=incoming[addr]*4+out
        ranked.append({'entry':addr,'name':fn.get('name',''), 'incoming_calls':incoming[addr],
                       'outgoing_calls':out,'static_priority_score':score})
    ranked.sort(key=lambda x:(-x['static_priority_score'],-x['incoming_calls'],x['entry']))
    report={'schema':1,'source':str(args.export),'source_functions':len(lookup),
            'functions_truncated':data.get('functions_truncated',False),
            'ranked':ranked[:args.limit],
            'warning':'Call graph centrality is a static heuristic, NOT runtime coverage or true feature importance.'}
    if args.output: write_json(args.output,report)
    print(json.dumps(report,indent=2,ensure_ascii=False));return 0


def export_graph(args):
    data=load_ghidra_export(args.export)
    if not 1 <= args.limit <= 10000: raise ValueError('Invalid graph limit')
    dst=Path(args.output).expanduser().resolve()
    if dst.exists(): raise ValueError('Graph output already exists')
    functions=data['functions'][:args.limit]
    nodes={f['entry'].lower():f.get('name','') for f in functions if isinstance(f.get('entry'),str)}
    def safe(v): return str(v).replace('\\','\\\\').replace('"','\\"').replace('\n',' ')
    lines=['digraph RustportNative {','  rankdir=LR;','  node [shape=box,fontname="monospace"];']
    for addr,name in nodes.items():
        lines.append(f'  "{safe(addr)}" [label="{safe(name)}\\n{safe(addr)}"];')
    edges=set()
    for f in functions:
        src=str(f.get('entry','')).lower()
        for edge in f.get('calls',[]):
            dest=str(edge.get('to','')).lower()
            if dest in nodes and (src,dest) not in edges:
                edges.add((src,dest))
                lines.append(f'  "{safe(src)}" -> "{safe(dest)}";')
    lines.append('}')
    dst.parent.mkdir(parents=True,exist_ok=True)
    dst.write_text('\n'.join(lines)+'\n',encoding='utf-8')
    print(json.dumps({'output':str(dst),'nodes':len(nodes),'internal_edges':len(edges),
                      'note':'Graph reflects heuristic Ghidra call references; not runtime call coverage'},indent=2));return 0

def load_cases(path):
    cases = json.loads(must_file(path).read_text(encoding='utf-8'))
    if not isinstance(cases,list) or not cases or len(cases) > 1000:
        raise ValueError('Cases must be a non-empty list of <=1000 items')
    seen=set()
    for case in cases:
        if not isinstance(case,dict) or set(case)-{'id','registers','expected'} or not isinstance(case.get('id'),str) or not re.fullmatch(r'[A-Za-z0-9_.-]{1,80}',case['id']):
            raise ValueError('Invalid case ID/fields')
        if case['id'] in seen: raise ValueError('Duplicate case ID')
        seen.add(case['id'])
        regs = case.get('registers',{})
        if not isinstance(regs,dict) or any(k not in ('rax','rdi','rsi','rdx','rcx','r8','r9','rbx','r10','r11') for k in regs):
            raise ValueError('Only explicitly allowed argument registers may be set; RIP/RSP are forbidden')
        if any(type(v) is not int or not 0 <= v < 2**64 for v in regs.values()):
            raise ValueError('Registers must be unsigned 64-bit integers')
        if 'expected' in case and (type(case['expected']) is not int or not 0 <= case['expected'] < 2**64):
            raise ValueError('Expected return must be unsigned 64-bit integer')
    return cases


def unicorn_probe(code, registers, instruction_limit, time_ms, result_register):
    try:
        from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_64, UC_HOOK_CODE, UC_HOOK_INTR
        from unicorn import x86_const as c
    except ImportError as e:
        raise RuntimeError('Unicorn 2.x Python binding not installed; pip install unicorn in your isolated environment') from e
    BASE = 0x100000
    STACK = 0x200000
    STOP = BASE + 0xF000
    PAGE = 0x1000
    # 64 KiB code + 64 KiB stack; no host OS or library calls are mapped.
    if len(code) > 0xE000: raise ValueError('Code too large for emulation region')
    mu = Uc(UC_ARCH_X86, UC_MODE_64)
    mu.mem_map(BASE, 0x10000)
    mu.mem_map(STACK, 0x10000)
    mu.mem_write(BASE, code)
    start_sp = STACK + 0xF000 - 8
    mu.reg_write(c.UC_X86_REG_RSP,start_sp)
    mu.mem_write(start_sp,STOP.to_bytes(8,'little'))
    name_to_id={n:getattr(c,'UC_X86_REG_'+n.upper()) for n in ('rax','rdi','rsi','rdx','rcx','r8','r9','rbx','r10','r11','rip','rsp')}
    for k,v in registers.items(): mu.reg_write(name_to_id[k],v)
    info={'instructions':0,'reason':None}
    def code_hook(em,address,size,_):
        info['instructions'] += 1
        if info['instructions'] > instruction_limit:
            info['reason']='instruction limit'; em.emu_stop(); return
        if not BASE <= address < BASE+len(code):
            info['reason']='instruction left extracted function'; em.emu_stop(); return
        inst=bytes(em.mem_read(address,min(size,8)))
        if inst.startswith((b'\x0f\x05',b'\x0f\x34',b'\xcd',b'\xcc',b'\xf4')):
            info['reason']='unsupported privileged/interrupt instruction'; em.emu_stop()
    def intr_hook(em,interrupt,_):
        info['reason']='software interrupt %s'%interrupt
        em.emu_stop()
    mu.hook_add(UC_HOOK_CODE,code_hook)
    mu.hook_add(UC_HOOK_INTR,intr_hook)
    try:
        mu.emu_start(BASE,STOP,timeout=time_ms*1000,count=instruction_limit+1)
    except UcError as e:
        raise RuntimeError('Unicorn stopped: '+str(e)) from e
    if info['reason']:
        raise RuntimeError('Unicorn rejected function: '+info['reason'])
    if mu.reg_read(c.UC_X86_REG_RIP)!=STOP or mu.reg_read(c.UC_X86_REG_RSP)!=start_sp+8:
        raise RuntimeError('Function did not return via RET to the test sentinel within resource limits')
    return {'value':mu.reg_read(name_to_id[result_register]),'instructions':info['instructions']}


def emulate(args):
    codepath = must_file(args.code)
    if not args.allow_emulation:
        raise ValueError('Use --allow-emulation to explicitly approve running raw bytes in Unicorn')
    size=codepath.stat().st_size
    if not 1 <= size <= 0xE000: raise ValueError('Raw function must be between 1 and 57344 bytes')
    if not 1 <= args.instruction_limit <= 100000 or not 1 <= args.timeout_ms <= 5000:
        raise ValueError('Invalid execution budget')
    if args.result_register not in ('rax','rbx','rcx','rdx','rdi','rsi','r8','r9','r10','r11'):
        raise ValueError('Unsupported return register')
    cases = load_cases(args.cases)
    dest = Path(args.output).expanduser().resolve()
    if dest.exists() or dest in (codepath,Path(args.cases).resolve()):
        raise ValueError('Refusing to overwrite emulation output')
    code = codepath.read_bytes()
    outputs=[]
    for case in cases:
        result=unicorn_probe(code,case.get('registers',{}),args.instruction_limit,args.timeout_ms,args.result_register)
        record={'id':case['id'],'registers':case.get('registers',{}), 'result':result['value']}
        if 'expected' in case and result['value']!=case['expected']:
            raise ValueError('Reference mismatch in case %s: expected %s, got %s'%(case['id'],case['expected'],result['value']))
        outputs.append(record)
    dest.parent.mkdir(parents=True,exist_ok=True)
    with dest.open('x',encoding='utf-8') as out:
        for o in outputs: out.write(json.dumps(o,sort_keys=True)+'\n')
    write_json(str(dest)+'.provenance.json',{'schema':1,'source_kind':'unicorn-emulated-raw-function-not-original-game-execution',
               'code_sha256':digest(codepath),'cases_sha256':digest(args.cases), 'results_sha256':digest(dest),
               'machine':'x86_64','instruction_limit':args.instruction_limit,'timeout_ms':args.timeout_ms,
               'result_register':args.result_register,'total_cases':len(cases)})
    print(json.dumps({'status':'emulated','cases':len(cases),'output':str(dest), 'sha256':digest(dest)},indent=2)); return 0


def cli():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest='command',required=True)
    sub.add_parser('doctor').set_defaults(func=doctor)
    a=sub.add_parser('analyze',help='Ghidra headless static analysis, requires explicit opt-in')
    a.add_argument('--binary',required=True);a.add_argument('--output',required=True);a.add_argument('--headless')
    a.add_argument('--max-functions',type=int,default=2000);a.add_argument('--max-edges',type=int,default=100)
    a.add_argument('--decompile-limit',type=int,default=0);a.add_argument('--timeout',type=int,default=900)
    a.add_argument('--allow-analysis',action='store_true');a.add_argument('--dry-run',action='store_true');a.set_defaults(func=analyze)
    s=sub.add_parser('slice',help='Extract raw file-offset bytes of a function; never publish extracted code')
    s.add_argument('--binary',required=True);s.add_argument('--offset',type=lambda x:int(x,0),required=True)
    s.add_argument('--length',type=lambda x:int(x,0),required=True);s.add_argument('--output',required=True)
    s.add_argument('--allow-slice',action='store_true');s.set_defaults(func=slice_bytes)
    i=sub.add_parser('ingest',help='Import Ghidra entry points as inferred, unverified ledger entries')
    i.add_argument('--export',required=True);i.add_argument('--db',default='port-ledger.sqlite');i.add_argument('--subsystem',default='native');i.add_argument('--limit',type=int,default=10000);i.set_defaults(func=ingest)
    r=sub.add_parser('rank',help='Rank function hotspots by inferred static call graph centrality')
    r.add_argument('--export',required=True);r.add_argument('--limit',type=int,default=30);r.add_argument('--output');r.set_defaults(func=rank_functions)
    gr=sub.add_parser('graph',help='Write a Graphviz DOT diagram from bounded recovered functions')
    gr.add_argument('--export',required=True);gr.add_argument('--limit',type=int,default=500);gr.add_argument('--output',required=True);gr.set_defaults(func=export_graph)
    e=sub.add_parser('emulate',help='Emulate explicit x86_64 raw function bytes using Unicorn 2.x')
    e.add_argument('--code',required=True);e.add_argument('--cases',required=True);e.add_argument('--output',required=True)
    e.add_argument('--instruction-limit',type=int,default=5000);e.add_argument('--timeout-ms',type=int,default=1000)
    e.add_argument('--result-register',default='rax');e.add_argument('--allow-emulation',action='store_true');e.set_defaults(func=emulate)
    return p


def main():
    args=cli().parse_args()
    try: return args.func(args)
    except (ValueError,OSError,RuntimeError,TimeoutError,sqlite3.Error,json.JSONDecodeError) as e:
        print('ERROR:',str(e),file=sys.stderr);return 2

if __name__=='__main__':raise SystemExit(main())
