"""Provider-switch regression tests. Copyright 2026 krylovim; see LICENSE."""
import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest

spec = importlib.util.spec_from_file_location("configure_plugin", Path(__file__).with_name("configure-plugin.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

SAMPLE = '''# keep this comment
model = "example"
[mcp_servers.bugboard]
command = "C:/private/server.exe"
args = ["--stdio"]
disabled_tools = ["bug_vote"]
[mcp_servers.bugboard.env]
BUGBOARD_PROFILE = "work"
[mcp_servers.other]
command = "untouched"
[mcp_servers.other.env]
KEY = "synthetic-not-a-real-secret"
[plugins."other@market"]
enabled = true
'''


class SwitchTests(unittest.TestCase):
    def test_prepare_install_rollback_and_reactivation(self):
        text = SAMPLE
        for mode, manual, plugin in [("prepare", False, False), ("plugin", False, True),
                                     ("standalone", True, False), ("plugin", False, True)]:
            text = module.transform(text, mode)
            data = tomllib.loads(text)
            self.assertEqual(data["mcp_servers"]["bugboard"]["enabled"], manual)
            self.assertEqual(data["plugins"][module.PLUGIN]["enabled"], plugin)
            self.assertIn("# keep this comment", text)
            self.assertEqual(module.transform(text, mode), text)

    def test_fresh_install_and_unsupported_configuration(self):
        data = tomllib.loads(module.transform('model="example"\n', "plugin"))
        self.assertNotIn("mcp_servers", data)
        self.assertTrue(data["plugins"][module.PLUGIN]["enabled"])
        with self.assertRaises(ValueError):
            module.transform('model="example"', "standalone")
        with self.assertRaises(ValueError):
            module.transform('[mcp_servers]\nbugboard = {command="x"}', "prepare")

    def test_atomic_switch_backup_and_git_refusal(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            config = root / "config.toml"
            config.write_text(SAMPLE, encoding="utf-8")
            backups = root / "private"
            backups.mkdir()
            outcome = module.write_configuration(config, backups, "plugin")
            self.assertEqual(Path(outcome["backup"]).read_text(), SAMPLE)
            self.assertEqual(list(backups.glob("*.tmp")), [])
            before = config.read_bytes()
            (backups / ".git").mkdir()
            with self.assertRaises(ValueError):
                module.write_configuration(config, backups, "standalone")
            self.assertEqual(config.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
