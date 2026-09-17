# Issue #6: packaging decision

Assessment by krylovim, 2026-09-18. License: Apache-2.0 + Commons Clause 1.0;
the original license and author notices remain applicable.

Keep this issue in backlog until the remaining live acceptance of #3–#5 is
complete. The current installation already separates the versioned executable,
profile/session data, and installed diagnostic skill. Creating a second MCP
registration now would add a conflicting connection without resolving the
remaining renewal acceptance. Fresh Chrome login and restart already passed;
packaging adds little immediate value to this working standalone installation.

## Proposed install/update boundary

- Package the versioned MCP, local authentication helper and a tested snapshot
  of `bugboard-diagnostics` from the practical-skills repository. Record the
  skill source commit and supported MCP schema in the package.
- Use the supported `.codex-plugin/plugin.json`, `skills/`, `scripts/` and
  companion `.mcp.json` layout when implementing packaging. Validate against
  the installed plugin-creator schema at that time; a layout assessment is
  not an installation or update acceptance test.
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

## Acceptance still required

A disposable clean installation, update preserving bindings and sessions,
search in two products, interactive login and renewal, absence of duplicate
MCP processes, and rollback to a compatible executable/skill combination.
The existing standalone installation remains the supported route meanwhile.

This follows the issue's explicit option to defer packaging when its current
value is small; no marketplace entry or plugin registration was created.
