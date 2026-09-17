# Browser login research and local prototype

Added by krylovim, 2026-09-18. Repository licensing and notices apply.

## Decision

Issue #4 uses a separate browser owned by a local helper. The user enters the
1C password and any additional verification in that window. Neither password
nor cookie goes through the model, an MCP argument/result, shell arguments,
clipboard, environment variable, diagnostic output or a repository file.

The current [official browser documentation](https://learn.chatgpt.com/docs/browser)
describes a built-in browser and optional full CDP developer access, including
an explicit approval requirement for full CDP. It does not document a supported
secret-transfer channel from a signed-in browser to a local MCP credential store.
The browser entry points exposed in this Codex session do not provide that
channel either. This is a limitation of the checked interfaces, not proof that
every version of Codex lacks such an API. No full CDP session was requested,
no existing browser profile was inspected, and no internal browser endpoints or
profile databases were reverse engineered.

`scripts/browser-login.cjs` therefore uses the documented Playwright APIs:

- [`chromium.launch`](https://playwright.dev/docs/api/class-browsertype#browser-type-launch)
  starts a separate browser, using installed Chrome by default. It does not attach
  to an existing browser, provide a user-data directory or expose a debugging
  TCP port.
- [`browser.newContext`](https://playwright.dev/docs/api/class-browsercontext)
  creates an isolated non-persistent context; no `storageState` file is written.
- [`context.cookies(url)`](https://playwright.dev/docs/api/class-browsercontext#browser-context-cookies)
  selects cookies applicable to the fixed Bugboard HTTPS URL. The helper applies
  an additional domain/path/expiry check before serializing a Cookie header.
- [`context.close`](https://playwright.dev/docs/api/class-browsercontext#browser-context-close)
  and browser close dispose the temporary session on exit.

Installed Edge and bundled Playwright 1.62.1 were available on the pilot Windows
machine. Playwright is an optional local dependency of this helper, not a new
dependency of the Rust MCP server.

For a reproducible separate installation (Node 20 or later), install the exact
validated version outside the repository. Chrome supplies the default browser binary; this
does not download a separate Chromium build or modify the main project's npm
dependencies:

```powershell
$browserHelperRoot = Join-Path $env:LOCALAPPDATA 'bugboard-mcp\browser-helper'
npm install --prefix $browserHelperRoot --save-exact --ignore-scripts playwright@1.62.1
$env:NODE_PATH = Join-Path $browserHelperRoot 'node_modules'
```

The existing bundled runtime can also be used without reinstalling dependencies;
resolve its paths through the host's documented dependency-location tool rather
than hard-coding a particular Codex cache path into this script.

## Run

Run in an interactive local terminal, using a **reviewed executable built with
the session-store CLI**. The older deployed executable without `auth import`
cannot save this flow. Playwright must be available through normal Node module
resolution or `NODE_PATH` pointing to an existing trusted installation. Do not
run this command through a tool that collects secrets or browser traces.

```powershell
node scripts/browser-login.cjs --mcp C:\path\to\bugboard-mcp.exe --profile default
```

The helper opens a separate Chrome window at `https://bugboard.1c.ru/`. Sign in
there directly; after Bugboard is usable, press Enter in the helper terminal.
No password or cookie is entered into the terminal. The helper obtains only
cookies applicable to Bugboard's root URL, then runs:

```text
bugboard-mcp auth import --stdin --profile default
```

The Cookie header travels through the child's anonymous stdin pipe only.
The child validates the session using the server's authenticated status request
before atomically saving through the protected session store. Failed validation
must preserve the previous record. A successful helper result is only
`{"status":"saved"}`; all child output is discarded. Activate the store in MCP
configuration separately according to the session-store instructions; importing
does not rewrite a working MCP configuration.

The helper inherits `BUGBOARD_SESSION_ROOT` if an isolated pilot store is desired.
It removes legacy cookie/session-file variables from the child environment.
The supported profile syntax is a conservative alphanumeric name with `_` or
`-`, up to 64 characters. `--channel msedge|chromium` and `--timeout 30..1800`
are optional; default timeout is ten minutes. A fresh browser context is created
on each launch. Reusing a saved MCP session is the store's job; browser login
intentionally starts fresh.

If browser automation is unavailable, the separately implemented
`bugboard-mcp auth import --profile default` hidden terminal prompt is the
explicit manual fallback. Never place a cookie in the command line or paste it
into a conversation.

## Lifecycle and limits

- Closing the login window, Ctrl+C, terminal EOF or timeout before confirmation
  cancels without calling the importer. Failed import keeps the browser open so
  the user can finish login and try again.
- Once Enter authorizes validation and save, that bounded transaction finishes
  even if the window is closed during it. Validation has a 90-second helper
  deadline; an interrupted/failed child near the commit boundary can leave an
  uncertain result, so inspect `auth status` before retrying. The previous record
  must never be partially overwritten.
- Browser/child error details, URLs, console messages and network logs are not
  forwarded. Output contains fixed status codes only. Downloads are disabled;
  no screenshots, HARs, tracing or storage-state exports are created.
- In-memory JavaScript strings cannot be reliably zeroed. This is local-process
  secret handling, not protection from a compromised Windows account, debugger
  or OS crash dump. The browser itself still creates temporary runtime files;
  no persistent authenticated profile is requested.
- The prototype only accepts root-path cookies for Bugboard or its parent
  `1c.ru`, excludes partitioned/expired cookies and caps the header at 32 KiB.
  A site change requiring a different cookie path or storage mechanism must be
  investigated explicitly, not worked around by copying all browser data.
- No OAuth support, automatic refresh or server-side logout is assumed. Deleting
  a local store record does not revoke an upstream session.

## Verification checkpoint

On 2026-09-18 the following passed:

```powershell
node --test scripts/browser-login.test.cjs
$env:BUGBOARD_BROWSER_SMOKE='1'
node --test scripts/browser-login.test.cjs
```

Twelve tests pass, including fixed-origin selection, safe options/environment,
anonymous child-pipe delivery, cancellation, rejection, delayed navigation,
quoted cookie values and omission of unsupported ancillary cookies. The optional
installed-Edge test starts two fresh headless contexts with synthetic cookies;
this remains separate from the real Chrome acceptance below.

The real Chrome pilot completed on 2026-09-18: the user entered 1C credentials
in the isolated browser, the helper received terminal confirmation, the importer
verified authentication, and the session was saved into DPAPI profile `work`.
The browser then closed normally. A new MCP process from the shared runtime,
with no legacy cookie/env-file source, passed all nine scoped-search acceptance
groups against that stored session. One separate startup hit a transient HTTPS
timeout; the subsequent read-only run passed.

The pilot exposed two helper defects which were fixed and regression-tested:
slow SSO navigation must not immediately close the window, and one unsupported
ancillary cookie must not reject an otherwise valid authenticated header. No
cookie values were inspected or printed to diagnose these failures.

Still unverified: natural expiry or upstream revocation followed by a user
re-login, and decryption under a different Windows identity. Existing working
sessions were not revoked to manufacture an expiry test. Rejection/cancellation,
record preservation and import-based renewal were tested independently; those
tests are not reported as observation of real session expiry.

The installed shared helper is `%LOCALAPPDATA%/bugboard-mcp/login.ps1` (Chrome
by default). It uses its own pinned Playwright installation, not Codex's internal
runtime or a previous task directory. It updates a profile after verification;
restart MCP to load changed credentials in an already running task.
