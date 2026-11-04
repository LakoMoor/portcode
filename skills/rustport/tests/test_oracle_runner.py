"""Exercise consent gating and deterministic dual-run using a tiny synthetic probe."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT/'scripts/oracle_runner.py'

class OracleRunnerTests(unittest.TestCase):
    def test_requires_explicit_execution_and_detects_mismatch(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            # Real execution, but the probed executable is this test's own Python interpreter.
            script=root/'probe.py'
            script.write_text('import json,sys; print(json.dumps({"tick":0,"value":int(sys.argv[1])}))\n')
            def scenario(port_value):
                data={'schema':1,'original_argv':[sys.executable,str(script),'1'],
                      'port_argv':[sys.executable,str(script),port_value], 'timeout_seconds':5}
                f=root/'scenario.json'; f.write_text(json.dumps(data)); return f
            sc=scenario('1')
            blocked=subprocess.run([sys.executable,str(RUNNER),str(sc),'--output',str(root/'blocked')],capture_output=True)
            self.assertNotEqual(blocked.returncode,0)
            result=subprocess.run([sys.executable,str(RUNNER),str(sc),'--output',str(root/'good'),
                                   '--allow-execute-original','--compare'],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertTrue((root/'good/parity.json').is_file())
            scenario('2')
            result=subprocess.run([sys.executable,str(RUNNER),str(sc),'--output',str(root/'bad'),
                                   '--allow-execute-original','--compare'],capture_output=True,text=True)
            self.assertEqual(result.returncode,1,result.stderr)
            self.assertIn('/0/value',(root/'bad/parity.json').read_text())

if __name__ == '__main__': unittest.main()
