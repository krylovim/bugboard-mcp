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

## Verified before activation

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

Final launcher verification and supported Codex registration are recorded here
after completion. The existing manual provider stays available for rollback.
No old task folder, profile, cache or repository binding is removed.
