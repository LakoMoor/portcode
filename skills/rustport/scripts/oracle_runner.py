#!/usr/bin/env python3
"""Explicit, bounded original-vs-port experiment runner. Does not sandbox untrusted games."""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys

MAX_STDOUT = 32 * 1024 * 1024
MAX_STDIN = 4 * 1024 * 1024


def fingerprint(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def run_side(label, args, base_dir, stdin_data, timeout):
    cmd = args[label + '_argv']
    if not isinstance(cmd, list) or not cmd or not all(isinstance(x, str) and x for x in cmd):
        raise ValueError(f'{label}_argv must be nonempty string array')
    executable = Path(cmd[0]).expanduser().resolve()
    if not executable.is_file():
        raise ValueError(f'{label} executable not found: {executable}')
    target = base_dir / label
    target.mkdir(parents=True, exist_ok=True)
    env = {key: os.environ[key] for key in ('PATH', 'LANG', 'LC_ALL', 'SYSTEMROOT', 'WINDIR') if key in os.environ}
    env['HOME'] = str(target)
    env['XDG_CONFIG_HOME'] = str(target / 'config')
    env['XDG_DATA_HOME'] = str(target / 'data')
    env['XDG_CACHE_HOME'] = str(target / 'cache')
    env['TMPDIR'] = str(target)
    extra = args.get('env', {})
    if not isinstance(extra, dict) or any(not isinstance(k, str) or not isinstance(v, str) for k, v in extra.items()):
        raise ValueError('env must be a string-string dictionary')
    if any(key in ('HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME') for key in extra):
        raise ValueError('experiment may not override scratch home directories')
    env.update(extra)
    try:
        # argv is explicit: never use shell=True or eval.
        p = subprocess.run(cmd, input=stdin_data, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, cwd=target, env=env,
                           timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        raise RuntimeError(f'{label} timed out after {timeout} seconds')
    if len(p.stdout) > MAX_STDOUT or len(p.stderr) > MAX_STDOUT:
        raise ValueError(f'{label} output exceeds 32 MiB limit')
    outpath = base_dir / (label + '.jsonl')
    def strict_json(line):
        def pairs(items):
            value = {}
            for key, val in items:
                if key in value:
                    raise ValueError(f'duplicate key: {key}')
                value[key] = val
            return value
        def reject(text):
            raise ValueError(f'non-finite value: {text}')
        return json.loads(line, object_pairs_hook=pairs, parse_constant=reject)
    try:
        parsed = [strict_json(line) for line in p.stdout.decode('utf-8').splitlines() if line.strip()]
    except (UnicodeDecodeError, json.JSONDecodeError) as err:
        raise ValueError(f'{label} stdout must be JSONL: {err}')
    if not parsed or not all(isinstance(v, dict) and v for v in parsed):
        raise ValueError(f'{label} emitted no structured state events')
    outpath.write_bytes(p.stdout)
    (base_dir / (label + '.stderr.txt')).write_bytes(p.stderr)
    return {'exit': p.returncode, 'argv0_sha256': fingerprint(executable),
            'sha256': fingerprint(outpath), 'events': len(parsed), 'output': str(outpath)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('scenario', type=Path, help='JSON scenario with original_argv and port_argv')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--allow-execute-original', action='store_true', help='Required explicit opt-in to run original')
    parser.add_argument('--compare', action='store_true', help='Run workbench parity after capture')
    opts = parser.parse_args()
    if not opts.allow_execute_original:
        parser.error('Running an original binary requires --allow-execute-original')
    scenario = json.loads(opts.scenario.read_text(encoding='utf-8'))
    if scenario.get('schema') != 1:
        parser.error('scenario schema must be 1')
    timeout = scenario.get('timeout_seconds', 15)
    if not isinstance(timeout, (int, float)) or not (0 < timeout <= 120):
        parser.error('timeout_seconds must be >0 and <=120')
    input_name = scenario.get('stdin_file')
    stdin_data = b''
    if input_name:
        # Resolve relative to scenario, not the caller's current directory.
        source = (opts.scenario.resolve().parent / input_name).resolve()
        if source.stat().st_size > MAX_STDIN:
            parser.error('stdin_file too large')
        stdin_data = source.read_bytes()
    output = opts.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error('output directory must be empty to avoid mixing captures')
    output.mkdir(parents=True, exist_ok=True)
    results = {}
    try:
        for side in ('original', 'port'):
            results[side] = run_side(side, scenario, output, stdin_data, timeout)
        if results['original']['exit'] != results['port']['exit']:
            raise ValueError('exit statuses differ')
        if results['original']['exit'] != 0:
            raise ValueError('both processes must exit successfully for a passing comparison')
        summary = {'schema': 1, 'original': results['original'], 'port': results['port'],
                   'note': 'These are captured outputs of two user-chosen executables. Independent reference provenance is not automatically certified.'}
        (output / 'capture.json').write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8')
        if opts.compare:
            compare = subprocess.run([sys.executable, str(Path(__file__).with_name('workbench.py')),
                                      'parity', str(output / 'original.jsonl'), str(output / 'port.jsonl'),
                                      '--report', str(output / 'parity.json')], check=False)
            return compare.returncode
        print(json.dumps(summary, indent=2))
        return 0
    except (OSError, ValueError, RuntimeError) as error:
        print(f'ERROR: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
