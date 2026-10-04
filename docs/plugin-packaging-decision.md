# Issue #6: packaging decision

Assessment by krylovim, 2026-09-18; implementation authorized 2026-10-04.
License: Apache-2.0 + Commons Clause 1.0;
the original license and author notices remain applicable.

The initial decision was to defer packaging. On 2026-10-04 the user explicitly
requested completion of issue #6. The saved profile was rejected by Bugboard;
the user repeated Chrome login, the helper verified and saved it, and a new
MCP process passed all nine search acceptance groups. This exercises renewal
of a previously working session after server rejection. It does not establish
whether rejection resulted from elapsed lifetime or server-side revocation.

The implementation uses a local Windows package with a committed skill snapshot,
the previously verified executable, browser helper and pinned dependencies.
It installs versioned code under the common application-data root; user data
stays outside code and plugin caches. See [installation](../plugin/README.md)
and the [current verification checkpoint](plugin-verification.md).

## Install/update boundary

- Package the versioned MCP, local authentication helper and a tested snapshot
  of `bugboard-diagnostics` from the practical-skills repository. Record the
  skill source commit and supported MCP schema in the package.
- Use the supported `.codex-plugin/plugin.json`, `skills/`, `scripts/` and
  companion `.mcp.json` compatibility layout, checked against current host CLI
  and installed examples. It remains documented in
  [official packaging guidance](https://developers.openai.com/plugins/build/plugins).
  A separate local marketplace supplies the prepared package.
- Keep profile credentials, catalog cache and installation backups in the
  user's application-data directory, outside plugin caches. Keep repository
  `.bugboard.json` files in their existing repositories.
- Before installing the plugin's MCP registration, detect the existing
  `mcp_servers.bugboard` entry. Prepare and validate a replacement, then switch
  once with an explicit reversible migration; never silently run both.
- Updating/uninstalling plugin code must not delete user sessions, caches or
  repository bindings. Local credential deletion stays a separate operation.
- Include LICENSE and retained author notices, mark fork changes, and preserve
  a NOTICE if one is introduced upstream. Do not label this fork pure Apache-2.0.

## Acceptance

A disposable clean installation, update preserving bindings and sessions,
search in two products, interactive login and renewal, absence of duplicate
MCP processes, and rollback to a compatible executable/skill combination.
The standalone installation remains available as a reversible fallback.

The distributable ZIP is currently local-use: the skill source repository has
no independent public redistribution license. Source packaging tools are kept
in this fork, while the ZIP and private skill snapshot are excluded from Git.
No public marketplace listing or public binary/skill release is implied.
