import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('rustport_install', ROOT / 'install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rustport-install-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.home = self.base / 'user home'
        self.project = self.base / 'project with spaces'

    def run_install(self, *args, ok=True):
        result = subprocess.run([sys.executable, str(ROOT / 'install.py'), '--home', str(self.home), *args],
                                capture_output=True, text=True)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def assert_bundle(self, destination):
        expected = {relative.as_posix(): source.read_bytes()
                    for source, relative in installer.bundle_files()}
        actual = {p.relative_to(destination).as_posix(): p.read_bytes()
                  for p in destination.rglob('*') if p.is_file()}
        self.assertEqual(actual, expected)

    def test_all_global_layouts_and_complete_bundles(self):
        self.run_install('--agent', 'all')
        roots = {'codex': '.agents', 'claude': '.claude', 'opencode': '.config/opencode',
                 'pi': '.pi/agent', 'dsh': '.dsh', 'hermes': '.hermes'}
        for agent, root in roots.items():
            with self.subTest(agent=agent):
                self.assert_bundle(self.home / root / 'skills/rustport')
                if agent in ('pi', 'opencode'):
                    folder = 'prompts' if agent == 'pi' else 'commands'
                    command = (self.home / root / folder / 'rustport.md').read_text()
                    self.assertIn(str(self.home / root / 'skills/rustport/SKILL.md'), command)
                    self.assertIn('$ARGUMENTS', command)
        self.assertFalse((self.home / '.codex/prompts').exists())
        self.assertEqual((self.home / '.agents/skills/rustport/LICENSE').read_bytes(),
                         (ROOT / 'LICENSE').read_bytes())
        self.assertTrue((self.home / '.agents/skills/rustport/NOTICE').is_file())

    def test_all_project_layouts(self):
        self.run_install('--all', '--root', str(self.project))
        for root in ('.agents', '.claude', '.opencode', '.pi', '.dsh', '.hermes'):
            self.assert_bundle(self.project / root / 'skills/rustport')
        self.assertFalse(self.home.exists())

    def test_default_preserves_claude_install(self):
        self.run_install()
        self.assert_bundle(self.home / '.claude/skills/rustport')
        self.assertFalse((self.home / '.agents').exists())

    def test_multiple_agents_only_install_selected(self):
        self.run_install('--agent', 'codex', '--agent', 'dsh')
        self.assert_bundle(self.home / '.agents/skills/rustport')
        self.assert_bundle(self.home / '.dsh/skills/rustport')
        self.assertFalse((self.home / '.claude').exists())

    def test_dry_run_writes_nothing(self):
        result = self.run_install('--all', '--dry-run')
        self.assertIn('nothing written', result.stdout)
        self.assertFalse(self.home.exists())

    def test_conflict_preflight_preserves_every_target(self):
        conflict = self.home / '.pi/agent/prompts/rustport.md'
        conflict.parent.mkdir(parents=True)
        conflict.write_text('user custom command')
        self.run_install('--all', ok=False)
        self.assertEqual(conflict.read_text(), 'user custom command')
        self.assertFalse((self.home / '.agents').exists())

    def test_force_only_replaces_named_bundle_files(self):
        skill = self.home / '.claude/skills/rustport'
        skill.mkdir(parents=True)
        (skill / 'SKILL.md').write_text('old')
        custom = skill / 'personal.txt'
        custom.write_text('keep')
        self.run_install('--force')
        self.assertEqual((skill / 'SKILL.md').read_bytes(), (ROOT / 'SKILL.md').read_bytes())
        self.assertEqual(custom.read_text(), 'keep')

    def test_identical_install_is_idempotent(self):
        self.run_install('--all')
        self.run_install('--all')

    def test_agent_config_is_preserved(self):
        config = self.home / '.hermes/config.yaml'
        config.parent.mkdir(parents=True)
        config.write_text('model: existing\n')
        self.run_install('--all')
        self.assertEqual(config.read_text(), 'model: existing\n')

    def test_symlink_destination_is_not_followed_even_with_force(self):
        outside = self.base / 'outside'
        outside.mkdir()
        self.home.mkdir()
        try:
            (self.home / '.claude').symlink_to(outside, target_is_directory=True)
        except OSError:
            self.skipTest('Symlink creation unavailable on this platform')
        self.run_install('--force', ok=False)
        self.assertEqual(list(outside.iterdir()), [])

    def test_unknown_agent_fails_without_writes(self):
        self.run_install('--agent', 'not-real', ok=False)
        self.assertFalse(self.home.exists())

    def test_home_overrides(self):
        for agent, variable in [('claude', 'CLAUDE_CONFIG_DIR'), ('pi', 'PI_CODING_AGENT_DIR'),
                                ('dsh', 'DSH_HOME'), ('hermes', 'HERMES_HOME')]:
            with self.subTest(agent=agent), patch.dict(os.environ, {variable: str(self.base / agent)}):
                skill, _ = installer.locations(agent, self.home)
                self.assertEqual(skill, self.base / agent / 'skills/rustport')
                ignored, _ = installer.locations(agent, self.home, use_env=False)
                self.assertTrue(ignored.is_relative_to(self.home))

    def test_opencode_xdg_override(self):
        with patch.dict(os.environ, {'XDG_CONFIG_HOME': str(self.base / 'xdg')}):
            skill, command = installer.locations('opencode', self.home)
        self.assertEqual(skill, self.base / 'xdg/opencode/skills/rustport')
        self.assertEqual(command, self.base / 'xdg/opencode/commands/rustport.md')

    def test_project_ignores_user_overrides(self):
        with patch.dict(os.environ, {'DSH_HOME': str(self.base / 'elsewhere')}):
            skill, _ = installer.locations('dsh', self.home, self.project)
        self.assertEqual(skill, self.project / '.dsh/skills/rustport')

    @unittest.skipIf(os.name == 'nt', 'Bash compatibility shim is POSIX-only')
    def test_shell_entrypoint_passes_arguments(self):
        result = subprocess.run(['bash', str(ROOT / 'install.sh'), '--home', str(self.home),
                                 '--agent', 'pi', '--dry-run'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('pi:', result.stdout)
        self.assertFalse(self.home.exists())


if __name__ == '__main__':
    unittest.main()
