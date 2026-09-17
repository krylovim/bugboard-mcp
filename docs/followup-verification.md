# Follow-up issues: verification checkpoint

Changes by krylovim, 2026-09-18. Apache-2.0 + Commons Clause 1.0 and original
author notices are retained. Development branch: `codex/catalog-auth-followup`.
Upstream PR #2 and its source branch are unchanged.

## Implemented

- #2: catalog selector cache, 24-hour TTL, explicit refresh, partial coverage,
  metadata-only discovery, account/session generation isolation and atomic writes.
- #5 foundations: local Windows DPAPI profiles, verified import/refresh, hidden
  input, safe status/delete, explicit selection without legacy fallback.
- #4: isolated browser helper using supported Playwright APIs, anonymous local
  pipe into the validating importer, cancellation and fixed status messages.
- #3 integration: installed diagnostic skill understands cache age, stale
  candidates and metadata-only results while retaining older-schema fallback.
  An independent pilot passed on actual BP and Trade demo-source exports.
- #6: assessed and retained in backlog pending remaining user-flow acceptance;
  no conflicting plugin/MCP registration was installed.

## Checks completed

On Windows with Rust 1.96.0 (GNU, `-C link-self-contained=yes`): formatting,
examples check, Clippy with warnings denied, 113 workspace/all-target tests,
one doctest, build and verified wire-crate packaging passed. Cargo-deny passed
advisories, licenses, bans and sources with the repository policy unchanged.

`scripts/followup-live.cjs` passed all six live groups against a fresh disposable
DPAPI profile, using the existing authorized external session without reading
its value into the test runner:

1. Verified env-to-DPAPI import and live authentication in a new process.
2. Empty/cancelled and rejected input preserve the previous ciphertext.
3. Corrupted protected session fails closed even with a valid legacy source set.
4. Repeated memory access, fresh disk access after restart, legacy-compatible
   live handles and forced catalog refresh.
5. BP and ERP scoped searches with the protected session.
6. Credential renewal creates a new isolated cache generation.

The disposable protected profile was locally deleted after each run. Deletion
does not revoke the session on 1C servers. The working MCP configuration and
legacy session were not changed by this acceptance script.

The existing `scripts/search-live.cjs` also passed all nine groups with the new
binary: BP/ERP isolation, description-only match, ambiguity, exact numbers,
special characters, limits and legacy calls.

## Acceptance boundaries

Synthetic DPAPI/ACL and browser lifecycle tests do not establish successful
fresh login, upstream expiry/revocation or decryption under another Windows
identity. Those observations are reported separately from deterministic tests.
Cache coverage remains partial and does not promise pagination or completeness.

Remote [CI on core 1287b94](https://github.com/krylovim/bugboard-mcp/actions/runs/35284315337)
passed Ubuntu and Windows verification plus Docker build. No image publication
is claimed: workflow dispatch skips publication. [Final code dbea3d4 CI](https://github.com/krylovim/bugboard-mcp/actions/runs/35284775332)
also passed all three jobs. Automatic push runs did not appear; manual dispatch
was used. Subsequent checkpoint changes affect documentation/comments only.

## Real browser and installed runtime

The user completed Chrome login in the helper's fresh context. Profile `work`
was saved only after live authentication. After browser closure a new process
using only DPAPI passed all nine search groups from the common runtime folder.
The helper has twelve passing tests, including installed-browser lifecycle.

Installed executable: `runtime/dbea3d47f145/bugboard-mcp.exe`, commit
`dbea3d47f145570cb9da4ea572b2ce0b7a59dd94`, SHA-256
`72A3DB3B6FFD101F5FB8990A505DD0AD4A66EA4BC31760949CE4EA2A27DB4069`.
This final build also passed all nine live groups. Two initial attempts encountered
HTTPS transport failures; both old/new CLI status checks remained authenticated,
and the subsequent complete search run passed without changing credentials.
The shared configuration selects `BUGBOARD_SESSION_STORE=dpapi` and profile
`work`. Its full parsed TOML matches the pre-switch backup except for the
intended Bugboard executable and auth settings; six disabled write tools remain.
Open tasks still need an MCP restart to replace their existing process/schema.

`login.ps1` and the browser helper live in the same common installation, with
Playwright 1.62.1 installed separately from Codex. The version-2 rollback helper
switches executable **and** compatible auth environment. A disposable TOML
fixture passed current→previous→current, preservation of unrelated settings,
and hash-mismatch rejection. Previous db9ac11 executable and private legacy env
remain for rollback; old task folders are retained.

## Diagnostic skill pilot

Published skill revision: `324f5e0` in the practical-skills repository. Six
binding tests and the system skill validator passed, including UTF-8 CLI output
under a legacy Windows code page. A separate agent validated actual exports of
BP 3.0.203.24 and Trade 11.5.27.75, mapped to `bp3` and `trade11` from source
metadata rather than the first catalog row. Both authorized read-only Designer
exports exited successfully; their source XML was neither modified nor committed.

Nine checks cover first binding, four alternating process-level resolutions,
version-only change, changed identity and duplicate ambiguity. Live searches in
the order BP→Trade→BP returned only the requested projects, with description-only
matches and explicit incomplete coverage. Card inspection did not establish a
cause or applicability to these releases. The broad catalogs had 8 and 4
candidates; metadata resolved them. A real user clarification dialogue was not
needed and is not claimed as tested. Rule-based clarification was assessed in
the control case without metadata.

Local evidence is in ignored `target/skill-pilot/binding-pilot-results.json` and
`live-pilot-results.json`; maintained findings are in the skill's
`references/validation.md`. Demo exports are real source inputs, not evidence
of deployment to production repositories.

## Remaining acceptance and issue disposition

Issues #1 and #2 meet their documented automated/live criteria. The #3 real demo
pilot passed; ambiguity without metadata was assessed as a control scenario,
not a live user dialogue. #4 has a working Chrome prototype and a documented
supported-API decision, with explicit expiry/revocation limits. #5 implementation
and migration are deployed, but full live acceptance remains open for natural
expiry/re-login, distinct real accounts and another Windows identity. Synthetic
rejection-after-success and profile-isolation regressions passed. #6 remains
intentionally deferred; no plugin installation or Docker publication is claimed.

No user input is required for the current working installation. Existing tasks
must restart their MCP process (reopen Codex if no per-server restart is exposed)
to use the updated schema and credentials. A future expired session is renewed
through the shared `login.ps1 -Profile work` helper, then an MCP restart.
