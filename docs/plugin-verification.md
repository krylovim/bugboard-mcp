# Windows plugin verification, 2026-10-04

Changes by krylovim. Apache-2.0 + Commons Clause 1.0 and original notices apply.

## Scope and sources

Issue #6 packages the existing verified Windows MCP (`dbea3d47f145570cb9da4ea572b2ce0b7a59dd94`)
with Chrome login, pinned Playwright 1.62.1 and a committed diagnostic skill.
The private skill source is recorded in each bundle's `skill-provenance.json`;
the package is for the owner's local use, not a public binary/skill release.
The source packaging tools and tests are publishable parts of this fork.

The [official plugin documentation](https://developers.openai.com/plugins/build/plugins)
still supports the compatibility manifest used here. Installed local examples
and `codex plugin --help` were checked on this host; registration uses the
supported CLI, not hand-created plugin cache records. Active runtime state and
data live outside Codex's disposable plugin cache.

## Automated verification

- Five builder tests: committed snapshot, file allowlist, tamper/extra-file
  rejection, deterministic ZIP, traversal and committed symlink rejection.
- Three provider-switch tests: prepare/plugin/standalone, unrelated settings,
  backup preservation, unsupported layout and refusal of Git backup directories.
- Nine registrar tests: no duplicate enabled provider during CLI steps,
  exact marketplace restoration on failure, preservation of unrelated entries,
  no raw child output, uncertain plugin-cache failures stay disabled.
- Ten Windows PowerShell 5.1 lifecycle groups: clean install, update, idempotence,
  profile/data preservation, rollback including corrupt current release,
  manifest/path/link refusal, current-user/SYSTEM ACL and skill/runtime mismatch.
- A native executable probe verifies UTF-8 Cyrillic/emoji JSON in both directions,
  continuous stdin, no extra transport output and exit-code forwarding.
- The complete 203-file prototype manifest verified in 1.874 seconds under
  Windows PowerShell 5.1; this is an observation on this host, not a time guarantee.

The first live launcher test exposed an inherited PowerShell 7 `PSModulePath`
that hid `Get-FileHash` from Windows PowerShell 5.1 when launched by Node.
Direct PowerShell invocations had normalized that environment and did not expose
the problem. Runtime checks now use .NET hashing directly and the regression
probe explicitly exercises the contaminated module path.

[CI run 37195092046](https://github.com/krylovim/bugboard-mcp/actions/runs/37195092046)
passed all four jobs on implementation commit
`c930975dbfff80a8af4fb7d3fc1d11144dd040ab`: full Rust verification on Ubuntu and
Windows, Windows plugin lifecycle and Docker build. The Docker job now waits
for both verification jobs and the plugin job. This feature-branch run did not
publish an image. The subsequent checkpoint commit changes documentation only.

CI totals: Windows 113 Rust unit/example tests and one doctest; Ubuntu 110
unit/example tests and one doctest. Dependency policy passed all four categories
on both systems. Plugin checks passed 17 Python tests, ten PowerShell lifecycle
groups and all five native stdio assertions. Browser-helper tests reported
11 passed, zero failed and one optional installed-browser smoke test skipped;
the user's live Chrome login is recorded separately below.

## Authentication acceptance

The previously saved `work` session returned `authenticated:false`. The user
completed fresh Chrome login, the existing helper validated and saved it, and
a new direct MCP process passed all nine search groups. A copied executable in
the prepared plugin installation also passed all nine groups. This tests renewal
after real server rejection without reading cookie values; the server's precise
expiry/revocation cause is unknown. Installation left protected ciphertext
unchanged. No working session was revoked merely for testing.

Multiple-profile isolation and decrypt-error behavior are covered by synthetic
DPAPI tests. A separate Windows identity and two real 1C accounts were not used.
DPAPI's current-user protection relies on the documented platform contract.

## Activation checkpoint

On 2026-10-04 the final PowerShell launcher passed all nine live search groups:
authentication, concurrent BP/ERP scopes, description-only matches, scope before
limit, invalid selectors, literal special characters, duplicate numbers and
legacy calls. Supported Codex CLI registration then completed successfully.

- Plugin: `bugboard@bugboard-local`, version `0.2.0`, installed and enabled.
- Shared release: `%LOCALAPPDATA%/bugboard-mcp/plugin-releases/0.2.0-dd190c8f168a`.
- Bundle manifest SHA-256:
  `dd190c8f168a2c5967abc44e6781f23b9de2cf63d39afade0a284ea6b4a6cf63`.
- Runtime source: `dbea3d47f145570cb9da4ea572b2ce0b7a59dd94`; executable SHA-256:
  `72a3db3b6ffd101f5fb8990a505dd0ad4a66ea4bc31760949ce4ea2a27db4069`.
- Diagnostic skill snapshot: `87224d88eb3058487057546b8a6eff7eaa4ca21f`.
- Local archive: `target/bugboard-plugin-0.2.0-final-windows.zip`; no public ZIP
  release, since the private skill has no independent redistribution grant.

All 203 files in the actual Codex plugin cache matched the installed manifest.
A fresh MCP launched from that cache initialized successfully, reported live
authentication and found product `bp3`. The local marketplace points at the
shared release, not at a previous task folder or the build output.

The parsed user configuration differs from its pre-activation backup only at
`marketplaces.bugboard-local`, `mcp_servers.bugboard.enabled` and
`plugins."bugboard@bugboard-local"`. The manual provider is disabled; the plugin
is enabled. Six disabled write tools are retained in both registrations.
Unrelated settings were verified unchanged. Old runtime files, profiles,
caches, task folders and repository bindings were retained.

The existing Codex task still holds its original MCP process. Restart Codex to
load the plugin in already-open tasks; this report verifies registration and
the installed transport independently, not a post-restart desktop session.
No other running task was interrupted to force the migration.

## Recovery and data retention

Use the shared active release's `scripts/login.ps1` to renew authorization in
Chrome. Use `scripts/rollback.ps1` to select the previous verified bundle, then
`scripts/register-plugin.py` if the skill snapshot changed, and restart MCP.
The launcher refuses a mismatched registered skill rather than mixing versions.

To return to the preserved standalone provider, run `scripts/configure-plugin.py`
with positional mode `standalone`, the existing Codex `--config` path and the shared
private `--backup-dir`. This enables the old manual entry and disables the
plugin together; restart Codex afterward. The standalone runtime uses the same
protected `work` profile.

Codex plugin removal removes its local cache. Shared runtime, DPAPI records and
catalog caches are outside that cache; repository bindings remain in their
repositories. Removal of the active plugin was not performed against the user's
working installation. Code rollback and data preservation were verified with
disposable lifecycle fixtures. Credential deletion remains a separate explicit
local operation and is not a server-side logout.
