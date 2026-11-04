import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT=Path(__file__).resolve().parents[1]/'scripts'/'differential_fuzz.py'

class FuzzTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        o=self.root/'oracle.py';r=self.root/'port.py'
        o.write_text('import json,sys\nd=json.loads(sys.stdin.readline())\nprint(json.dumps({"sum":d["a"]+d["b"]}))\n')
        r.write_text('import json,sys\nd=json.loads(sys.stdin.readline())\nprint(json.dumps({"sum":d["a"]+d["b"]+(d["a"]>2)}))\n')
        self.original=[sys.executable,str(o)];self.port=[sys.executable,str(r)]
        self.manifest=self.root/'manifest.json'
        self.manifest.write_text(json.dumps({'schema':1,'original':self.original,'port':self.port,
                                             'fields':{'a':{'min':3,'max':8},'b':{'min':0,'max':10}}}))
    def call(self,*args,rc=0):
        p=subprocess.run([sys.executable,str(SCRIPT),str(self.manifest),'--output',str(self.root/'out'),*args],capture_output=True,text=True)
        self.assertEqual(p.returncode,rc,'stdout:'+p.stdout+' stderr:'+p.stderr)
        return p
    def test_explicit_consent(self):
        self.call(rc=2)
        self.assertFalse((self.root/'out').exists())
    def test_seeded_repro_and_minimize(self):
        p=self.call('--iterations','10','--seed','123','--minimize','--allow-execute-original',rc=1)
        report=json.loads((self.root/'out'/'report.json').read_text())
        self.assertEqual(report['status'],'DIFF')
        self.assertEqual(report['executed_cases'],1)
        self.assertIn('reduced_input',report['first_difference'])
        self.assertEqual(len((self.root/'out'/'original.jsonl').read_text().splitlines()),1)
    def test_parity_success_is_bounded(self):
        manifest=json.loads(self.manifest.read_text());manifest['port']=manifest['original'][:]
        # Commands must differ, so use a second distinct but equivalent program.
        r=self.root/'port_same.py';r.write_text('import json,sys\na=json.loads(sys.stdin.readline());print(json.dumps({"sum":a["a"]+a["b"]}))\n')
        manifest['port']=[sys.executable,str(r)];self.manifest.write_text(json.dumps(manifest))
        self.call('--iterations','4','--allow-execute-original')
        report=json.loads((self.root/'out'/'report.json').read_text())
        self.assertEqual(report['status'],'PASS')
        self.assertEqual(report['executed_cases'],4)
    def test_malformed_manifest_fails(self):
        self.manifest.write_text('{"schema":1,"schema":1}')
        self.call('--allow-execute-original',rc=2)

if __name__=='__main__':unittest.main()
