"""Register the installed Bugboard release using the supported Codex CLI.

Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
Run after install.ps1 (also after rollback when the skill revision changes).
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid

spec = importlib.util.spec_from_file_location("provider", Path(__file__).with_name("configure-plugin.py"))
provider = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provider)


def run(args):
    result = subprocess.run(args, capture_output=True, timeout=180)
    if result.returncode:
        raise ValueError("Host command failed")
    return result


def replace_private(path, data):
    temp = path.with_name(f"{path.name}-{uuid.uuid4().hex}.tmp")
    try:
        with temp.open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--installation-root", type=Path,
                        default=Path(os.environ.get("LOCALAPPDATA", "")) / "bugboard-mcp")
    parser.add_argument("--codex", default="codex")
    args = parser.parse_args()
    prepared = False
    config = Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex"))) / "config.toml"
    restore_mode = "prepare"
    old_marketplace = None
    marketplace_changed = False
    host_attempted = False
    try:
        root = args.installation_root.resolve(strict=True)
        codex = shutil.which(args.codex)
        if not codex:
            raise ValueError("Codex CLI missing")
        state = json.loads((root / "plugin-installation.json").read_text(encoding="utf-8-sig"))
        entry = state["current"]
        release_id = entry["release_id"]
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}-[a-f0-9]{12}", release_id):
            raise ValueError("Invalid active release")
        release = root / "plugin-releases" / release_id
        manifest = (release / "bundle-manifest.json").read_bytes()
        if hashlib.sha256(manifest).hexdigest() != entry["manifest_sha256"]:
            raise ValueError("Invalid active release")
        # Existing trusted installer verifies the complete inventory, including
        # links/ACLs and hashes, before the provider configuration changes.
        run(["powershell.exe", "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass",
             "-File", str(release / "scripts/install.ps1"), "-PackageRoot", str(release),
             "-InstallationRoot", str(root)])
        backup_dir = root / "backups"
        import tomllib
        before = tomllib.loads(config.read_text(encoding="utf-8-sig"))
        old_manual = before.get("mcp_servers", {}).get("bugboard")
        old_plugin = before.get("plugins", {}).get(provider.PLUGIN, {}).get("enabled", False)
        if old_manual is not None and old_manual.get("enabled", True):
            restore_mode = "standalone"
        elif old_plugin:
            restore_mode = "plugin"
        marketplace = {"name": "bugboard-local", "interface": {"displayName": "Bugboard local"},
                       "plugins": [{"name": "bugboard", "source": {"source": "local", "path": f"./plugin-releases/{release_id}"},
                                    "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
                                    "category": "Productivity"}]}
        folder = root / ".agents/plugins"
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / "marketplace.json"
        if path.exists():
            old_marketplace = path.read_bytes()
            existing = json.loads(old_marketplace.decode("utf-8-sig"))
            if existing["name"] != "bugboard-local":
                raise ValueError("Refusing to overwrite another marketplace")
            matches = [entry for entry in existing["plugins"] if entry["name"] == "bugboard"]
            if len(matches) != 1:
                raise ValueError("Ambiguous Bugboard marketplace entry")
            matches[0]["source"] = marketplace["plugins"][0]["source"]
            marketplace = existing
        # No fresh task can start two enabled providers during CLI installation.
        provider.write_configuration(config, backup_dir, "prepare")
        prepared = True
        replace_private(path, json.dumps(marketplace, indent=2).encode("utf-8"))
        marketplace_changed = True
        host_attempted = True
        run([codex, "plugin", "marketplace", "add", str(root), "--json"])
        run([codex, "plugin", "add", provider.PLUGIN, "--json"])
        provider.write_configuration(config, backup_dir, "plugin")
        print(json.dumps({"status": "registered", "plugin": provider.PLUGIN,
                          "release_id": release_id, "restart_required": True}))
    except (ValueError, KeyError, OSError, subprocess.SubprocessError):
        recovered = not prepared
        marketplace_restored = True
        if marketplace_changed:
            try:
                if old_marketplace is None:
                    path.unlink(missing_ok=True)
                else:
                    replace_private(path, old_marketplace)
            except OSError:
                marketplace_restored = False
        if prepared:
            try:
                # CLI may have replaced the cached skill before returning an
                # error. Do not re-enable an uncertain old plugin/runtime pair.
                safe_mode = "prepare" if host_attempted and restore_mode == "plugin" else restore_mode
                provider.write_configuration(config, root / "backups", safe_mode)
                recovered = safe_mode == restore_mode
            except (ValueError, OSError):
                pass
        recovered = recovered and marketplace_restored
        print(json.dumps({"status": "registration_failed", "previous_provider_restored": recovered}))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
