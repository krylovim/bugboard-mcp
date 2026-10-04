"""Switch one Bugboard provider in Codex without copying or displaying secrets.

Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
Python 3.11+; install the package and verify its stdio launcher before activation.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import re
import tempfile
import tomllib
import uuid

PLUGIN = "bugboard@bugboard-local"


def set_enabled(text, section, enabled, *, required=False):
    header = f"[{section}]"
    pattern = re.compile(r"(?m)^" + re.escape(header) + r"[ \t]*(?:#[^\r\n]*)?\r?$")
    matches = list(pattern.finditer(text))
    if not matches:
        if required:
            raise ValueError("Unsupported or absent standalone Bugboard table")
        return text.rstrip() + f"\n\n{header}\nenabled = {str(enabled).lower()}\n"
    if len(matches) != 1:
        raise ValueError("Ambiguous provider table")
    start = matches[0].end()
    next_table = re.search(r"(?m)^\s*\[", text[start:])
    end = start + next_table.start() if next_table else len(text)
    body = text[start:end]
    existing = re.compile(r"(?m)^[ \t]*enabled[ \t]*=.*$")
    value = f"enabled = {str(enabled).lower()}"
    if existing.search(body):
        body = existing.sub(value, body)
    else:
        body = "\n" + value + body if body.startswith("\n") else "\n" + value + "\n" + body
    return text[:start] + body + text[end:]


def transform(text, mode):
    original = tomllib.loads(text)
    expected = copy.deepcopy(original)
    standalone = expected.get("mcp_servers", {}).get("bugboard")
    if mode == "standalone" and standalone is None:
        raise ValueError("No standalone connection to restore")
    result = text
    if standalone is not None:
        standalone["enabled"] = mode == "standalone"
        result = set_enabled(result, "mcp_servers.bugboard", mode == "standalone", required=True)
    expected.setdefault("plugins", {}).setdefault(PLUGIN, {})["enabled"] = mode == "plugin"
    result = set_enabled(result, f'plugins."{PLUGIN}"', mode == "plugin")
    if tomllib.loads(result) != expected:
        raise ValueError("Unexpected configuration difference; nothing written")
    return result


def write_configuration(config, backup_dir, mode):
    config = config.resolve(strict=True)
    backup_dir = backup_dir.resolve(strict=True)
    if not config.is_file() or not backup_dir.is_dir():
        raise ValueError("Existing config and private backup directory required")
    if os.stat(config.parent).st_dev != os.stat(backup_dir).st_dev:
        raise ValueError("Private backup directory must be on the configuration volume for atomic replacement")
    # Config backups can contain credentials from unrelated connections.
    if any((p / ".git").exists() for p in [backup_dir, *backup_dir.parents]):
        raise ValueError("Configuration backups must stay outside Git")
    before = config.read_bytes()
    after = transform(before.decode("utf-8-sig"), mode).encode("utf-8")
    if before == after:
        return {"mode": mode, "changed": False}
    backup = backup_dir / f"config-before-plugin-{uuid.uuid4().hex}.toml"
    with backup.open("xb") as stream:
        stream.write(before)
    descriptor, temp_name = tempfile.mkstemp(prefix="config-plugin-", suffix=".tmp", dir=backup_dir)
    temp = Path(temp_name)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(after)
            stream.flush()
            os.fsync(stream.fileno())
        if config.read_bytes() != before:
            raise ValueError("Configuration changed concurrently; nothing written")
        os.replace(temp, config)
    finally:
        temp.unlink(missing_ok=True)
    return {"mode": mode, "changed": True, "backup": str(backup)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["prepare", "plugin", "standalone"])
    parser.add_argument("--config", type=Path, default=Path.home() / ".codex/config.toml")
    parser.add_argument("--backup-dir", type=Path, required=True,
                        help="Existing private directory outside Git, on the same volume as config.toml")
    args = parser.parse_args()
    try:
        print(json.dumps(write_configuration(args.config, args.backup_dir, args.mode)))
    except (ValueError, OSError):
        # Do not echo TOML parser exceptions or configuration source lines.
        print(json.dumps({"status": "error", "message": "Provider switch failed; inspect the local configuration and backup directory without printing secrets."}))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
