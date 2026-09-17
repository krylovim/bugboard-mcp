# Shared local runtime

Initial deployment checkpoint by krylovim, 2026-09-17. Supersedes the pending
deployment step in the earlier issue #1 verification report.

**Updated 2026-09-18:** the active installation now uses runtime `1287b94c76e6`,
Windows DPAPI profile `work` and the shared Chrome `login.ps1` helper. See the
[current acceptance and migration checkpoint](followup-verification.md).
The history below records the first deployment; the installed v2 rollback
helper now switches both executable and compatible authorization, with db9ac11
as the previous version. The legacy env is retained only for that rollback.

Fork `main` was fast-forwarded to `db9ac110eb8e40b499ddfcf508d0065052b15659`,
including the inherited text-search fix exactly once. Upstream PR #2 and its
source branch were not changed by this merge.

The user installation now lives outside chat/project folders:

```text
%LOCALAPPDATA%/bugboard-mcp/
  runtime/db9ac110eb8e/bugboard-mcp.exe
  runtime/e5b9ff289881/bugboard-mcp.exe  (independent rollback)
  session/bugboard.env
  backups/
  installation.json
  rollback-runtime.ps1
  LICENSE
  README.md
```

Codex's existing `mcp_servers.bugboard` entry points to the first binary and
the shared session file. Only `command` and `BUGBOARD_SESSION_ENV` changed;
stdio args, timeouts and the six disabled write tools were preserved. A backup
of the original config is stored in the private installation directory.

The session was copied without printing its contents. Windows ACL grants
access to the installing user and SYSTEM only; this is not DPAPI encryption
or protection from other processes of the same user. The prior task folder
is retained as a safety copy but is no longer required by configured launches.

## Verification

- Runtime SHA-256: `27138FDC32D415776C726F25FA6EEAB819A27A6153F4E4405181CF3B819EEE92`.
- Copied session equality checked without exposing the value or its hash.
- All 9 groups in `scripts/search-live.cjs` passed using the shared paths.
- An independent stdio process launched with the shared directory as cwd
  exposed `project_code`/`mode` and queried the live catalog successfully.
- Separate BP, BSP, technology-platform and mobile-platform searches returned
  only the requested project; see the diagnostic skill's validation notes.
- `codex mcp get bugboard` confirms the new paths and preserved disabled tools.
- Rollback and return were tested against an isolated configuration, including
  preservation of unrelated settings and the session path.

Open tasks still holding an older MCP process need a restart. If the app does
not expose a per-server restart, close and reopen Codex. Processes were not
force-killed during migration. A new
configuration entry does not retroactively replace the tool schema in an
already running task. See the [official MCP setup documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).

## Rollback

The installed helper switches only the Bugboard executable, preserves the
shared session and unrelated config, checks the executable's hash and makes
an atomic configuration backup:

```powershell
& "$env:LOCALAPPDATA/bugboard-mcp/rollback-runtime.ps1" -Version Previous
# To return to the scoped-search build:
& "$env:LOCALAPPDATA/bugboard-mcp/rollback-runtime.ps1" -Version Current
```

Restart Bugboard MCP in open tasks after either switch. These are manual
recovery commands, not automatic fallback on transport failures.

## Diagnostic skill

`bugboard-diagnostics` is maintained in the user's practical-skills repository
and installed via `~/.codex/skills/bugboard-diagnostics`. It resolves a product
from repository metadata, keeps local bindings, and explicitly separates
product, BSP and platform search hypotheses. This does not implement the MCP
catalog cache from issue #2 or the authentication work from issues #4/#5.
