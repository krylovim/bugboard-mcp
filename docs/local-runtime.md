# Windows runtime operations

Updated by krylovim, 2026-10-30. Apache-2.0 + Commons Clause 1.0 and original
notices apply. The user reports this installation is in production.

## Active installation

Plugin `bugboard@bugboard-local` version `0.2.0` is enabled. Its release
`0.2.0-dd190c8f168a` contains runtime `dbea3d47f145` and uses DPAPI profile
`work`. The manual `mcp_servers.bugboard` entry is disabled and retained as a
standalone fallback. Six write tools remain disabled.

Code and user data live outside this repository:

```text
%LOCALAPPDATA%/bugboard-mcp/
  plugin-installation.json          active and previous release selectors
  plugin-releases/<release-id>/     immutable executable, skill and login helper
  protected-sessions/               DPAPI-protected profiles
  backups/                         private configuration backups
  runtime/                         retained standalone runtimes
  installation.json                standalone installation/rollback metadata
```

Catalog cache namespaces are also outside the worktree; see
[catalog storage](catalog-cache.md). Repository product bindings remain in each
consumer repository. Codex's plugin cache is a separate installed copy, not the
source of session data. Do not remove the shared installation or its retained
rollback resources while cleaning a development worktree.

## Login and recovery

Run the active release's `scripts/login.ps1` from a local terminal. It opens
Chrome, validates the authenticated session and saves it under the selected
profile. Enter credentials only in the browser. See the [login procedure](browser-login-research.md).
Restart the MCP connection after renewing a session so a new process loads it.
If no per-server restart is available, close and reopen Codex.

For a code update, run the new package's `scripts/install.ps1`, verify its
launcher, then run `scripts/register-plugin.py`. For rollback, use
`scripts/rollback.ps1`; refresh registration if the skill snapshot changed.
The launcher rejects a registered skill that differs from the active bundle.
See [installation and rollback](../plugin/README.md) and the
[verified release baseline](plugin-verification.md).

To restore the retained standalone provider, use `scripts/configure-plugin.py`
with positional mode `standalone`, the intended `--config` file and the shared
private `--backup-dir`. This switches the two providers together. Restart MCP
afterward. Do not enable both providers separately.

## Development cleanup

The repository contains source, regression tests, build/install tooling and
current operational/API documentation. Local `target/` contents are disposable:
compiler caches, package copies, probes and temporary demo database exports.
Python `__pycache__/` directories are also disposable. Rebuild packages with
`scripts/build-plugin.py`; production launch does not depend on those outputs.

On 2026-10-30 the installed release and Codex cache passed all 203 manifest file
checks before cleanup. No new live Bugboard query was needed for documentation
and artifact cleanup; the last recorded live acceptance is dated 2026-10-04.
Historical issue reports and migration drafts remain recoverable in Git history.
