# Browser login

Maintained by krylovim, updated 2026-10-30. Repository licensing and notices apply.

The local helper opens a separate Chrome window. The user enters the 1C
password and any additional verification there. Passwords and cookies are not
returned to the agent, copied to the clipboard or written into the repository.
The helper uses an isolated, non-persistent Playwright context and does not
inspect an existing browser profile.

## Installed plugin

Run the active release's `scripts/login.ps1` from an interactive local terminal.
For the default shared installation:

```powershell
$bugboardRoot = Join-Path $env:LOCALAPPDATA 'bugboard-mcp'
$bugboardState = Get-Content -LiteralPath (Join-Path $bugboardRoot 'plugin-installation.json') -Raw | ConvertFrom-Json
$bugboardRelease = Join-Path $bugboardRoot ('plugin-releases/' + $bugboardState.current.release_id)
& (Join-Path $bugboardRelease 'scripts/login.ps1')
```

The script verifies the release and selects its profile, defaulting to `work`.
It requires Node.js 20+ and installed Google Chrome; pinned Playwright 1.62.1 is
included in the installed release. No separate browser download is needed.

1. Sign in at Bugboard in the new Chrome window.
2. Wait until the working Bugboard page opens, then press Enter in the helper
   terminal. Do not enter a password or cookie into that terminal.
3. Wait for `{"status":"saved"}`. The helper closes the browser after saving.
4. Restart the MCP connection and check `bugboard_auth_status` before searching.

The helper sends only Bugboard cookies through an anonymous stdin pipe to
`bugboard-mcp auth import --stdin --profile <profile>`. The importer validates
live authentication before atomically replacing the DPAPI-protected record.
Failed validation preserves the previous record. Existing MCP processes keep
their in-memory session until restarted.

## Standalone helper

For an explicitly selected reviewed binary supporting `auth import`, use:

```powershell
node scripts/browser-login.cjs --mcp C:/path/to/bugboard-mcp.exe --profile work
```

Playwright 1.62.1 must be resolvable by Node, normally from the helper's own
installation. The optional `--channel msedge` selects installed Edge;
`--timeout` accepts 30–1800 seconds, default 600. An absolute
`BUGBOARD_SESSION_ROOT` may select another dedicated protected store.
Import does not rewrite the MCP configuration; see [protected storage](session-storage.md).

## Cancellation and limits

- Closing the window, Ctrl+C, terminal EOF or timeout before confirmation
  cancels without importing. A failed import leaves the window open for retry.
- Once Enter starts validation/save, the bounded transaction may finish even
  if the browser is closed. If interrupted near that boundary, check auth
  status before retrying; do not assume the previous record was replaced.
- Output uses fixed status messages. No screenshots, browser traces, HAR files
  or persistent authenticated browser profile are requested.
- Only supported, unexpired root-path cookies for Bugboard or its parent
  `1c.ru` are accepted; the header limit is 32 KiB. A site change needs review,
  not indiscriminate copying of browser data.
- This does not provide OAuth, unattended renewal or server-side logout.
  Local session deletion does not revoke the server session.
- DPAPI and ACLs protect storage from other users, not malware/debuggers running
  under the same Windows account. In-memory strings cannot be reliably zeroed.

Automated lifecycle checks are in `scripts/browser-login.test.cjs`. Fresh Chrome
login, renewal after server rejection and search in a new process passed during
[release acceptance](plugin-verification.md). The exact expiry/revocation cause
and decryption under another Windows identity were not established.
