"""Local synthetic contract tests; no real Ghidra or Unicorn required."""
import json
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest

NATIVE=Path(__file__).resolve().parents[1]/'scripts'/'native_lab.py'
WORKBENCH=Path(__file__).resolve().parents[1]/'scripts'/'workbench.py'

class NativeLabTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.binary=self.root/'original.bin'
        self.binary.write_bytes(bytes.fromhex('8d0437c3')+b'\x90'*128)
    def run_native(self,*args,exitcode=0):
        p=subprocess.run([sys.executable,str(NATIVE),*map(str,args)],capture_output=True,text=True)
        self.assertEqual(p.returncode,exitcode,f'OUT={p.stdout}\nERR={p.stderr}')
        return p
    def run_workbench(self,*args,exitcode=0):
        p=subprocess.run([sys.executable,str(WORKBENCH),*map(str,args)],capture_output=True,text=True)
        self.assertEqual(p.returncode,exitcode,f'OUT={p.stdout}\nERR={p.stderr}')
        return json.loads(p.stdout)
    def test_doctor_does_not_execute_file(self):
        d=json.loads(self.run_native('doctor').stdout)
        self.assertIn('unicorn_version',d)
        self.assertTrue(d['ghidra_script'].endswith('RustportExport.java'))
    def test_slice_requires_optin_and_provenance(self):
        out=self.root/'slice.bin'
        self.run_native('slice','--binary',self.binary,'--offset','0x0','--length',4,'--output',out,exitcode=2)
        self.assertFalse(out.exists())
        self.run_native('slice','--binary',self.binary,'--offset',0,'--length',4,'--output',out,'--allow-slice')
        self.assertEqual(out.read_bytes(),bytes.fromhex('8d0437c3'))
        meta=json.loads(Path(str(out)+'.provenance.json').read_text())
        self.assertEqual(meta['file_offset'],0)
        self.assertEqual(meta['length'],4)
        self.run_native('slice','--binary',self.binary,'--offset',0,'--length',4,'--output',out,'--allow-slice',exitcode=2)
        self.run_native('slice','--binary',self.binary,'--offset',200,'--length',4,'--output',self.root/'bad','--allow-slice',exitcode=2)
    @unittest.skipIf(os.name=='nt','synthetic executable mock needs POSIX shebang')
    def test_fake_headless_contract_and_ingest(self):
        mock=self.root/'mock-headless'
        # Test the actual analyzeHeadless CLI construction and provenance logic with a synthetic fake.
        mock.write_text('#!'+sys.executable+'\n'+'''import json,sys
from pathlib import Path
args=sys.argv[1:]
assert '-import' in args and '-scriptPath' in args and '-postScript' in args, args
s=args.index('-postScript')
assert args[s+1]=='RustportExport.java'
out=Path(args[s+2]); out.parent.mkdir(parents=True,exist_ok=True)
out.write_text(json.dumps({'schema':1,'tool':'ghidra-headless','functions':[{'entry':'00401000','name':'step','calls':[{'from':'00401004','to':'00403000'}]}, {'entry':'00402000','name':'tick','calls':[]}], 'functions_truncated':False}))
print('RUSTPORT_EXPORT_OK')
''')
        mock.chmod(0o755)
        export=self.root/'ghidra.json'
        pre=self.run_native('analyze','--binary',self.binary,'--output',export,'--headless',mock,exitcode=2)
        self.assertIn('requires --allow-analysis',pre.stderr)
        self.assertFalse(export.exists())
        preview=self.run_native('analyze','--binary',self.binary,'--output',export,'--headless',mock,'--dry-run')
        self.assertIn('command_preview',preview.stdout)
        self.assertFalse(export.exists())
        self.run_native('analyze','--binary',self.binary,'--output',export,'--headless',mock,'--allow-analysis')
        data=json.loads(export.read_text())
        self.assertEqual(len(data['functions']),2)
        self.assertEqual(len(data['source']['sha256']),64)
        db=self.root/'ledger.sqlite'
        a=json.loads(self.run_native('ingest','--export',export,'--db',db).stdout)
        self.assertEqual(a['added'],2)
        b=json.loads(self.run_native('ingest','--export',export,'--db',db).stdout)
        self.assertEqual(b['skipped_existing'],2)
        stats=self.run_workbench('ledger','--db',db,'stats')
        self.assertEqual(stats['total'],2)
        self.assertEqual(stats['statuses']['unknown'],2)
        rows=self.run_workbench('ledger','--db',db,'list')
        self.assertEqual(rows[0]['confidence'],'inferred')
        rank=json.loads(self.run_native('rank','--export',export,'--limit',5).stdout)
        self.assertEqual(len(rank['ranked']),2)
        graph=self.root/'graph.dot'
        self.run_native('graph','--export',export,'--output',graph)
        self.assertIn('digraph RustportNative',graph.read_text())

    def test_empty_export_or_invalid_sha_cannot_ingest(self):
        path=self.root/'export.json'; db=self.root/'db.sqlite'
        path.write_text(json.dumps({'schema':1,'tool':'ghidra-headless','source':{'sha256':'x'*64},'functions':[{'entry':'00401000'}]}))
        self.run_native('ingest','--export',path,'--db',db,exitcode=2)
        path.write_text(json.dumps({'schema':1,'tool':'ghidra-headless','source':{'sha256':'0'*64},'functions':[]}))
        self.run_native('ingest','--export',path,'--db',db,exitcode=2)
    def test_emulator_cases_validation_and_optin(self):
        code=self.root/'code.bin';code.write_bytes(bytes.fromhex('8d0437c3'))
        cases=self.root/'cases.json';cases.write_text(json.dumps([{'id':'add','registers':{'rdi':2,'rsi':3},'expected':5}]))
        output=self.root/'result.jsonl'
        self.run_native('emulate','--code',code,'--cases',cases,'--output',output,exitcode=2)
        self.assertFalse(output.exists())
        cases.write_text(json.dumps([{'id':'bad','registers':{'rsp':0}}]))
        self.run_native('emulate','--code',code,'--cases',cases,'--output',output,'--allow-emulation',exitcode=2)
        self.assertFalse(output.exists())
        cases.write_text(json.dumps([{'id':'add','registers':{'rdi':2,'rsi':3},'expected':5}]))
        try: import unicorn
        except ImportError:
            result=self.run_native('emulate','--code',code,'--cases',cases,'--output',output,'--allow-emulation',exitcode=2)
            self.assertIn('Unicorn 2.x',result.stderr)
            self.assertFalse(output.exists())
        else:
            self.run_native('emulate','--code',code,'--cases',cases,'--output',output,'--allow-emulation')
            record=json.loads(output.read_text().strip())
            self.assertEqual(record['result'],5)
    @unittest.skipIf(os.name=='nt','synthetic shell mock needs POSIX')
    def test_decompiler_arguments_bound(self):
        mock=self.root/'fake';mock.write_text('#!/bin/sh\nexit 0\n');mock.chmod(0o755)
        r=self.run_native('analyze','--binary',self.binary,'--output',self.root/'out.json','--headless',mock,'--max-functions','0','--allow-analysis',exitcode=2)
        self.assertIn('out of range',r.stderr)
        self.run_native('analyze','--binary',self.binary,'--output',self.root/'out.json','--headless',mock,'--allow-analysis',exitcode=2)
    @unittest.skipIf(os.name=='nt','POSIX-only test fake headless executable')
    def test_headless_timeout_kills_process(self):
        mock=self.root/'sleep-headless';mock.write_text('#!/bin/sh\nsleep 30\n');mock.chmod(0o755)
        import importlib.util
        spec=importlib.util.spec_from_file_location('rustport_native_lab',NATIVE)
        mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
        with self.assertRaises(TimeoutError):
            mod.run_bounded([str(mock)],0.1)

if __name__=='__main__':unittest.main()
