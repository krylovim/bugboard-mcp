# Bugboard local plugin for Windows

Added by krylovim, 2026. This package preserves the Bugboard MCP license:
Apache-2.0 with Commons Clause 1.0; see `LICENSE` and `ATTRIBUTION.md`.

The plugin searches known 1C errors in explicitly selected product, BSP and
platform projects. It includes a Windows MCP executable, a committed snapshot
of the diagnostics skill, a local browser-login helper and pinned Playwright
dependencies. Runtime queries work without npm or downloads after installation;
the Bugboard site still requires network access and an authenticated session.

## Requirements

- Windows under the user account that owns the DPAPI-protected session.
- Windows PowerShell 5.1 and Git for the skill's local repository bindings.
- Python 3.11 or later for the configuration migrator (the skill's binding
  helper alone requires Python 3.9 or later).
- Node.js 20 or later and installed Google Chrome for browser login. Installed
  Edge can be used with the browser helper's explicit `--channel msedge` option.
  No browser binary is bundled or downloaded during package installation.
- A Codex version supporting local plugin marketplaces and the compatibility
  `.codex-plugin/plugin.json` manifest.

## Install and register

Keep the extracted package together: its root contains `.agents/plugins/marketplace.json`
and the `plugin` directory. First verify/install it with the included script:

```powershell
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File C:\path\package\plugin\scripts\install.ps1
python C:\path\package\plugin\scripts\register-plugin.py
```

Check the installed launcher's authorization status before registration. The
registration helper points a local marketplace at the verified shared release,
disables both providers, then invokes the supported CLI commands
`codex plugin marketplace add <shared-installation-root>` and
`codex plugin add bugboard@bugboard-local`. It enables the plugin provider and
attempts to restore the previous provider if registration fails. Configuration
backups stay in the installation's private `backups` directory. Do not run
two independently configured Bugboard servers accidentally. Installation does
not silently remove a working standalone connection or copy its plaintext
credentials. Restart the MCP connection after a reviewed switch and verify the
authorization status and a scoped query.

The launch script reads an explicit shared installation state outside the
plugin cache. Session profiles, product caches and active runtime state belong
under `%LOCALAPPDATA%\bugboard-mcp`; never place secrets inside this package.
Install/launch/login/rollback parameters are documented by each PowerShell
script's parameter block. An optional `-InstallationRoot` selects an isolated
pilot installation. The default profile is `work`; explicit profiles keep
separate protected records and cache namespaces.

## Login and rollback

```powershell
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File C:\path\package\plugin\scripts\login.ps1
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File C:\path\package\plugin\scripts\rollback.ps1
```

Enter the 1C password only in the dedicated browser window. The helper sends
the session through a local stdin pipe to the validating DPAPI importer; no
cookie is returned to the model or written to the package. Validation must
succeed before an existing protected session is replaced. Local deletion is
not upstream revocation. Rollback changes runtime selection, not credentials.
After an update or rollback that changes the skill commit, run
`register-plugin.py` again so the registered skill matches the active runtime
bundle, then restart MCP. A stale skill registration is rejected by the launcher.

## Provenance and distribution

`bundle-manifest.json` lists hashes for every file in `plugin` except itself,
plus the caller-supplied binary source commit and the committed skill snapshot.
Hashes detect accidental changes; the manifest is not a signature or independent
proof that a binary was compiled from that commit. Obtain the package and source
revision through a trusted channel.

This is a local-use package for the repository owner. The practical-skills
repository did not contain an independent public redistribution license when
checked. `skill-provenance.json` records that limitation; do not upload this ZIP
to a public release before resolving the skill's distribution terms. Playwright
and its dependencies retain their included third-party license notices.
