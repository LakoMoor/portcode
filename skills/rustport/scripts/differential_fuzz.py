#!/usr/bin/env python3
"""Reproducible bounded differential testing of explicit original/port JSON stdin harnesses.

Runs REAL original-side and Rust-side programs only with --allow-execute-original.
Not a sandbox, and neither a general game loader nor a proof of entire-game parity.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import random
import re
import subprocess
import sys
import tempfile
from workbench import strict_json, compare, sha256


def write_json(path,obj):
    p=Path(path);p.parent.mkdir(parents=True,exist_ok=True)
    p.write_text(json.dumps(obj,indent=2,ensure_ascii=False,sort_keys=True)+'\n',encoding='utf-8')


def checked_command(cmd):
    if not isinstance(cmd,list) or not cmd or not all(isinstance(x,str) and x and '\x00' not in x for x in cmd):
        raise ValueError('Commands must be nonempty argv lists; shell strings are forbidden')
    if len(cmd)>40:raise ValueError('Too many command arguments')
    return cmd


def parse_manifest(path):
    data=strict_json(Path(path).read_text(encoding='utf-8'))
    if not isinstance(data,dict) or data.get('schema')!=1:raise ValueError('expected schema 1')
    orig=checked_command(data.get('original'))
    port=checked_command(data.get('port'))
    if orig==port:raise ValueError('Original and port commands cannot be identical')
    fields=data.get('fields')
    if not isinstance(fields,dict) or not 1 <= len(fields) <= 20:raise ValueError('fields must contain 1..20 keys')
    for key,cfg in fields.items():
        if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,48}',key):raise ValueError('invalid field name')
        if not isinstance(cfg,dict) or set(cfg)!={'min','max'}:raise ValueError('field specification must be {"min":int,"max":int}')
        a,b=cfg['min'],cfg['max']
        if type(a)!=int or type(b)!=int or not -2**53<=a<=b<=2**53:raise ValueError('field bounds invalid')
    return orig,port,fields


def run_harness(cmd,case,timeout,home):
    env={k:os.environ[k] for k in ('PATH','LANG','LC_ALL','SYSTEMROOT','WINDIR') if k in os.environ}
    env.update({'HOME':str(home),'TMPDIR':str(home),'RUSTPORT_DIFFERENTIAL':'1'})
    try:
        p=subprocess.run(cmd,input=json.dumps(case,sort_keys=True)+'\n',capture_output=True,text=True,
                         errors='replace',timeout=timeout,cwd=str(home),env=env,check=False)
    except subprocess.TimeoutExpired:
        return {'error':'timeout'}
    except OSError as exc:
        return {'error':'launch failed: '+str(exc)}
    if p.returncode!=0:return {'error':'nonzero exit','code':p.returncode,'stderr_excerpt':p.stderr[:250]}
    try:
        result=strict_json(p.stdout.strip())
    except (ValueError,json.JSONDecodeError):
        return {'error':'invalid output (must be one JSON value)'}
    if not isinstance(result,dict) or not result:
        return {'error':'empty or non-object JSON output'}
    return {'result':result}


def differential(original,port,case,timeout,root):
    # Isolated scratch folders are useful for reproducibility but NOT security boundaries.
    with tempfile.TemporaryDirectory(prefix='rustport-fuzz-',dir=str(root)) as td:
        td=Path(td);(td/'original').mkdir();(td/'port').mkdir()
        a=run_harness(original,case,timeout,td/'original')
        b=run_harness(port,case,timeout,td/'port')
        ok='result' in a and 'result' in b and not compare(a['result'],b['result'],abs_tol=0,rel_tol=0,ignore=[])
        return ok,a,b


def minimize(original,port,case,timeout,root,cap=28):
    shrunk=dict(case);attempts=0
    for field in sorted(shrunk):
        v=shrunk[field]
        options=[0,1 if v>=0 else -1, v//2]
        for candidate in options:
            if candidate==shrunk[field] or attempts>=cap:continue
            trial=dict(shrunk);trial[field]=candidate;attempts+=1
            passed,a,b=differential(original,port,trial,timeout,root)
            # A broken original cannot establish behavior; minimize only actual divergences.
            if 'result' in a and not passed:
                shrunk=trial
    return shrunk,attempts


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('manifest');ap.add_argument('--output',required=True)
    ap.add_argument('--seed',type=int,default=0);ap.add_argument('--iterations',type=int,default=30)
    ap.add_argument('--timeout',type=float,default=2.0)
    ap.add_argument('--minimize',action='store_true');ap.add_argument('--allow-execute-original',action='store_true')
    args=ap.parse_args()
    try:
        if not args.allow_execute_original:raise ValueError('Explicit --allow-execute-original required; this is not a security sandbox')
        if not 1 <= args.iterations <= 1000 or not 0.1 <= args.timeout <= 30.0:
            raise ValueError('Invalid iteration or timeout limit')
        orig,port,fields=parse_manifest(args.manifest)
        root=Path(args.output).expanduser().resolve()
        if root.exists() and any(root.iterdir()):raise ValueError('Output directory must be empty or absent')
        root.mkdir(parents=True,exist_ok=True)
        rng=random.Random(args.seed)
        original_traces=[];port_traces=[];cases=[];first=None
        for i in range(args.iterations):
            case={k:rng.randint(v['min'],v['max']) for k,v in sorted(fields.items())}
            passed,a,b=differential(orig,port,case,args.timeout,root)
            if 'result' not in a:
                raise ValueError('Original oracle failed at case %d: %s'%(i,a))
            cases.append({'id':i,'input':case})
            original_traces.append({'id':i,'input':case,**a})
            port_traces.append({'id':i,'input':case,**b})
            if not passed:
                first={'id':i,'input':case,'original':a,'port':b}
                if args.minimize and 'result' in a:
                    small,attempts=minimize(orig,port,case,args.timeout,root)
                    first['reduced_input']=small;first['minimization_runs']=attempts
                break
        for path,items in [('original.jsonl',original_traces),('port.jsonl',port_traces)]:
            with (root/path).open('x',encoding='utf-8') as f:
                for record in items:f.write(json.dumps(record,ensure_ascii=False,sort_keys=True)+'\n')
        write_json(root/'cases.json',cases)
        summary={'schema':1,'seed':args.seed,'requested_cases':args.iterations,'executed_cases':len(cases),
                 'status':'PASS' if not first else 'DIFF','first_difference':first,
                 'manifest_sha256':sha256(args.manifest),
                 'oracle_sha256':sha256(root/'original.jsonl'),'port_sha256':sha256(root/'port.jsonl'),
                 'warning':'Not a sandbox. Parity covers generated cases only; verify original provenance and harness fidelity.'}
        write_json(root/'report.json',summary)
        print(json.dumps(summary,ensure_ascii=False,indent=2))
        return 0 if not first else 1
    except (ValueError,OSError,json.JSONDecodeError) as err:
        print('ERROR:',err,file=sys.stderr);return 2

if __name__=='__main__':raise SystemExit(main())
