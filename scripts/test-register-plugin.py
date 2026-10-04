"""Registration regression fixtures; never touch the user's Codex configuration.

Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
"""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("register_plugin", Path(__file__).with_name("register-plugin.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

CONFIG = '''# unrelated text stays
model = "synthetic-model"
[mcp_servers.bugboard]
command = "C:/example/bugboard.exe"
enabled = true
[mcp_servers.bugboard.env]
BUGBOARD_PROFILE = "test-profile"
[mcp_servers.other]
command = "other.exe"
[mcp_servers.other.env]
KEY = "synthetic-secret-never-output"
[plugins."other@market"]
enabled = true
'''
OLD_MARKETPLACE = b'{"name":"bugboard-local","plugins":[{"name":"bugboard","source":{"source":"local","path":"./old-release"}}]}\n'


class Fixture:
    def __init__(self, root):
        self.home = root / "home"
        self.config = self.home / ".codex/config.toml"
        self.config.parent.mkdir(parents=True)
        self.config.write_text(CONFIG, encoding="utf-8")
        self.root = root / "installation"
        (self.root / "backups").mkdir(parents=True)
        self.release_id = "0.2.0-111111111111"
        self.release = self.root / "plugin-releases" / self.release_id
        self.release.mkdir(parents=True)
        self.manifest = b'{"version":"0.2.0","files":{}}'
        (self.release / "bundle-manifest.json").write_bytes(self.manifest)
        (self.root / "plugin-installation.json").write_text(json.dumps({"schema": 1, "current": {
            "release_id": self.release_id,
            "manifest_sha256": hashlib.sha256(self.manifest).hexdigest(),
        }}), encoding="utf-8")
        self.marketplace = self.root / ".agents/plugins/marketplace.json"
        self.marketplace.parent.mkdir(parents=True)
        self.marketplace.write_bytes(OLD_MARKETPLACE)
        self.calls = []

    def invoke(self, fail=None):
        output = io.StringIO()

        def host(args):
            self.calls.append(args)
            if "plugin" in args:
                parsed = tomllib.loads(self.config.read_text(encoding="utf-8"))
                # During registration, neither provider may start a second server.
                assert parsed["mcp_servers"]["bugboard"]["enabled"] is False
                assert parsed["plugins"][module.provider.PLUGIN]["enabled"] is False
            if fail and fail(args):
                raise ValueError("synthetic-secret-never-output")
            return subprocess.CompletedProcess(args, 0, b"synthetic-hidden-child-output", b"")

        with patch.object(module.Path, "home", return_value=self.home), \
                patch.object(module.os, "environ", {"LOCALAPPDATA": str(self.root.parent)}), \
                patch.object(module.shutil, "which", return_value="C:/example/codex.exe"), \
                patch.object(module, "run", side_effect=host), \
                patch.object(sys, "argv", ["register-plugin.py", "--installation-root", str(self.root)]), \
                contextlib.redirect_stdout(output):
            try:
                module.main()
                code = 0
            except SystemExit as exc:
                code = exc.code
        printed = output.getvalue()
        assert "synthetic-secret-never-output" not in printed
        assert "synthetic-hidden-child-output" not in printed
        return code, json.loads(printed)


class RegistrationTests(unittest.TestCase):
    def test_success_preserves_unrelated_config_and_keeps_one_provider(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            code, result = f.invoke()
            self.assertEqual(code, 0)
            self.assertEqual(result["status"], "registered")
            data = tomllib.loads(f.config.read_text(encoding="utf-8"))
            self.assertFalse(data["mcp_servers"]["bugboard"]["enabled"])
            self.assertTrue(data["plugins"][module.provider.PLUGIN]["enabled"])
            self.assertEqual(data["mcp_servers"]["other"]["env"]["KEY"], "synthetic-secret-never-output")
            self.assertTrue(data["plugins"]["other@market"]["enabled"])
            self.assertIn("# unrelated text stays", f.config.read_text(encoding="utf-8"))
            self.assertEqual(len([c for c in f.calls if "plugin" in c]), 2)

    def test_failed_marketplace_add_restores_mapping_and_standalone(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            code, result = f.invoke(lambda args: "marketplace" in args)
            self.assertEqual(code, 1)
            self.assertTrue(result["previous_provider_restored"])
            self.assertEqual(f.marketplace.read_bytes(), OLD_MARKETPLACE)
            data = tomllib.loads(f.config.read_text(encoding="utf-8"))
            self.assertTrue(data["mcp_servers"]["bugboard"]["enabled"])
            self.assertFalse(data["plugins"][module.provider.PLUGIN]["enabled"])

    def test_failed_plugin_add_restores_mapping_and_standalone(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            code, result = f.invoke(lambda args: "plugin" in args and "marketplace" not in args)
            self.assertEqual(code, 1)
            self.assertTrue(result["previous_provider_restored"])
            self.assertEqual(f.marketplace.read_bytes(), OLD_MARKETPLACE)
            data = tomllib.loads(f.config.read_text(encoding="utf-8"))
            self.assertTrue(data["mcp_servers"]["bugboard"]["enabled"])

    def test_prepare_rejection_leaves_marketplace_and_config_unchanged(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            original = f.config.read_bytes()
            with patch.object(module.provider, "write_configuration", side_effect=ValueError("rejected")):
                code, result = f.invoke()
            self.assertEqual(code, 1)
            self.assertEqual(f.config.read_bytes(), original)
            self.assertEqual(f.marketplace.read_bytes(), OLD_MARKETPLACE)
            self.assertFalse(any("plugin" in args for args in f.calls))

    def test_bad_manifest_fails_before_host_and_provider_changes(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            (f.release / "bundle-manifest.json").write_bytes(b"corrupt")
            original = f.config.read_bytes()
            code, result = f.invoke()
            self.assertEqual(code, 1)
            self.assertEqual(f.calls, [])
            self.assertEqual(f.config.read_bytes(), original)
            self.assertEqual(f.marketplace.read_bytes(), OLD_MARKETPLACE)

    def test_host_failure_never_includes_child_output(self):
        result = subprocess.CompletedProcess(["codex"], 1, b"secret stdout", b"secret stderr")
        with patch.object(module.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "^Host command failed$"):
                module.run(["codex"])

    def test_old_plugin_recovery_stays_disabled_when_cli_outcome_is_uncertain(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            f.config.write_text(module.provider.transform(CONFIG, "plugin"), encoding="utf-8")
            code, result = f.invoke(lambda args: "plugin" in args and "marketplace" not in args)
            self.assertEqual(code, 1)
            self.assertFalse(result["previous_provider_restored"])
            self.assertEqual(f.marketplace.read_bytes(), OLD_MARKETPLACE)
            data = tomllib.loads(f.config.read_text(encoding="utf-8"))
            self.assertFalse(data["mcp_servers"]["bugboard"]["enabled"])
            self.assertFalse(data["plugins"][module.provider.PLUGIN]["enabled"])

    def test_new_marketplace_is_removed_on_failure(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            f.marketplace.unlink()
            code, result = f.invoke(lambda args: "marketplace" in args)
            self.assertEqual(code, 1)
            self.assertTrue(result["previous_provider_restored"])
            self.assertFalse(f.marketplace.exists())
            self.assertEqual(list(f.marketplace.parent.glob("*.tmp")), [])

    def test_existing_marketplace_unrelated_entries_and_fields_survive(self):
        with tempfile.TemporaryDirectory() as name:
            f = Fixture(Path(name))
            existing = json.loads(OLD_MARKETPLACE)
            existing["interface"] = {"displayName": "Keep this title"}
            existing["plugins"].append({"name": "other", "source": {"source": "local", "path": "./other"}})
            existing["plugins"][0]["policy"] = {"installation": "AVAILABLE"}
            f.marketplace.write_text(json.dumps(existing), encoding="utf-8")
            code, _ = f.invoke()
            self.assertEqual(code, 0)
            actual = json.loads(f.marketplace.read_text(encoding="utf-8"))
            existing["plugins"][0]["source"]["path"] = f"./plugin-releases/{f.release_id}"
            self.assertEqual(actual, existing)


if __name__ == "__main__":
    unittest.main()
