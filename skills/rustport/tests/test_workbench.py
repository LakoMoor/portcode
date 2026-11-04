"""Run with python3 -m unittest discover -s tests -v from package root."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOL=Path(__file__).resolve().parents[1]/'scripts/workbench.py'

class CliTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)

    def run_cli(self,*args,rc=0):
        p=subprocess.run([sys.executable,str(TOOL),*map(str,args)],text=True,capture_output=True)
        self.assertEqual(p.returncode,rc,msg=p.stdout+'\n'+p.stderr)
        return json.loads(p.stdout) if p.stdout.strip() else None

    def test_doctor(self):
        out=self.run_cli('doctor')
        self.assertTrue(out['python']['ok'])

    def test_init_no_clobber_and_ledger(self):
        project=self.root/'demo'
        self.run_cli('init','--project',project,'--name','Example','--source-version','2026')
        self.assertTrue((project/'docs/SCOPE.md').is_file())
        self.assertFalse((project/'Cargo.toml').exists())
        (project/'docs/SCOPE.md').write_text('CUSTOM')
        self.run_cli('init','--project',project,'--name','EXAMPLE','--source-version','2026')
        self.assertEqual((project/'docs/SCOPE.md').read_text(),'CUSTOM')
        db=project/'port-ledger.sqlite'
        self.run_cli('ledger','--db',db,'add','--id','physics.001','--origin','exe:0x01','--target','game-core::step','--evidence','docs/TRACES.md')
        s=self.run_cli('ledger','--db',db,'stats')
        self.assertEqual(s['total'],1)
        self.run_cli('ledger','--db',db,'set','--id','physics.001','--status','verified',rc=2)

    def test_parity_pass_and_stale(self):
        p=self.root/'p'
        self.run_cli('init','--project',p,'--name','T')
        db=p/'port-ledger.sqlite'
        self.run_cli('ledger','--db',db,'add','--id','drive.step','--origin','ref.dll:0xa','--target','game-core::step','--evidence','docs/TRACES.md')
        a=p/'oracle.jsonl'; b=p/'port.jsonl'
        a.write_text('{"tick":0,"x":1}\n{"tick":1,"x":2}\n')
        b.write_text('{"tick":0,"x":1}\n{"tick":1,"x":2}\n')
        result=self.run_cli('parity',a,b,'--db',db,'--id','drive.step')
        self.assertEqual(result['status'],'PASS')
        self.assertEqual(self.run_cli('ledger','--db',db,'stats')['statuses']['compared'],1)
        self.assertTrue(self.run_cli('ledger','--db',db,'audit')['pass'])
        b.write_text('{"tick":0,"x":1}\n{"tick":1,"x":3}\n')
        self.assertFalse(self.run_cli('ledger','--db',db,'audit',rc=1)['pass'])
        result=self.run_cli('parity',a,b,rc=1)
        self.assertEqual(result['differences'][0]['path'],'/1/x')

    def test_parity_tolerance_and_ignores(self):
        a=self.root/'a.json'; b=self.root/'b.json'
        a.write_text(json.dumps({'x':1.0,'meta':{'ts':1}}))
        b.write_text(json.dumps({'x':1.0001,'meta':{'ts':2}}))
        self.run_cli('parity',a,b,rc=1)
        r=self.run_cli('parity',a,b,'--abs-tol','0.001','--ignore','/meta/ts')
        self.assertTrue(r['pass'])
        self.run_cli('parity',a,a,rc=2)

    def test_inspect_static_only(self):
        binary=self.root/'test.exe'
        binary.write_bytes(b'MZ'+b'\x00'*10)
        out=self.root/'inspect.json'
        data=self.run_cli('inspect','--binary',binary,'--output',out)
        self.assertEqual(data['magic_or_extension'],'PE/DOS executable (MZ header)')
        self.assertEqual(len(data['sha256']),64)
        self.assertTrue(out.exists())

    def test_inventory_no_copy(self):
        game=self.root/'owned-game'; game.mkdir()
        (game/'a.bin').write_bytes(b'MZ'+b'\x00'*32)
        (game/'foo.txt').write_text('sample')
        target=self.root/'manifest.json'
        out=self.run_cli('inventory','--source',game,'--output',target)
        self.assertEqual(out['total_listed'],2)
        manifest=json.loads(target.read_text())
        self.assertEqual(manifest['source_label'],'owned-game')
        self.assertEqual(manifest['files'][0]['kind'],'PE/DOS executable (MZ header)')
        self.assertFalse(any('source_path' in e for e in manifest['files']))


    def test_empty_trace_and_duplicate_keys_are_rejected(self):
        a=self.root/'a.jsonl'; b=self.root/'b.jsonl'
        a.write_text(''); b.write_text('')
        self.run_cli('parity',a,b,rc=2)
        a.write_text('{"tick":1,"tick":2}\n'); b.write_text('{"tick":1}\n')
        self.run_cli('parity',a,b,rc=2)
        a.write_text('{"tick":1,"x":NaN}\n')
        self.run_cli('parity',a,b,rc=2)

    def test_lock_wired_verification_and_original_change(self):
        root=self.root/'game'; self.run_cli('init','--project',root,'--name','T')
        original=self.root/'original.exe'; original.write_bytes(b'MZ-test-binary')
        lock=root/'reference.lock.json'; self.run_cli('pin','--original',original,'--output',lock,'--version','v1')
        db=root/'port-ledger.sqlite'
        self.run_cli('ledger','--db',db,'add','--id','ai.step','--origin','v1:0x100','--target','sim::ai','--evidence','docs/TRACES.md')
        a=root/'a.jsonl'; b=root/'b.jsonl'
        a.write_text('{"tick":1,"x":0}\n'); b.write_text(a.read_text())
        self.run_cli('parity',a,b,'--db',db,'--id','ai.step','--reference-lock',lock,rc=2)
        self.run_cli('ledger','--db',db,'set','--id','ai.step','--wired','1','--status','implemented')
        result=self.run_cli('parity',a,b,'--db',db,'--id','ai.step','--reference-lock',lock)
        self.assertEqual(result['ledger_status'],'verified')
        self.assertTrue(self.run_cli('ledger','--db',db,'audit')['pass'])
        original.write_bytes(b'MZ-tampered')
        self.assertFalse(self.run_cli('ledger','--db',db,'audit',rc=1)['pass'])
        self.run_cli('parity',a,b,'--db',db,'--id','ai.step','--reference-lock',lock,rc=2)

    def test_ledger_edits_invalidate_proof(self):
        root=self.root/'p'; self.run_cli('init','--project',root,'--name','T')
        db=root/'port-ledger.sqlite'
        self.run_cli('ledger','--db',db,'add','--id','input','--origin','version','--target','input_fn','--evidence','docs/TRACES.md')
        a=root/'a.jsonl'; b=root/'b.jsonl'
        a.write_text('{"tick":1}\n'); b.write_text(a.read_text())
        self.run_cli('parity',a,b,'--db',db,'--id','input')
        self.run_cli('ledger','--db',db,'set','--id','input','--target','input_fn_v2')
        out=self.run_cli('ledger','--db',db,'show','--id','input')
        self.assertEqual(out['status'],'implemented')
        self.assertEqual(out['oracle_sha256'],'')

if __name__=='__main__':unittest.main()
