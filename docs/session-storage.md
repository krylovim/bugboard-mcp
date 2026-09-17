# Protected session profiles

Added by krylovim, 2026. Apache-2.0 + Commons Clause 1.0; existing notices apply.

Issue #5 foundations: a local CLI imports an authenticated session, validates it against
Bugboard, then persists only current-user Windows DPAPI ciphertext. This does not implement
SSO or automate MFA. The [browser-login research](browser-login-research.md) supplies a
separate interactive helper; a real fresh-login acceptance still requires the user.

## Local commands

Run the built executable locally, not as MCP tool calls:

```powershell
# Existing authorized environment/file, without printing its contents:
$env:BUGBOARD_SESSION_ENV = 'C:\private\bugboard.env'
.\bugboard-mcp.exe auth import-env --profile work

# Interactive hidden prompt; empty input cancels:
.\bugboard-mcp.exe auth import --profile work

# For a local browser helper only: raw Cookie header through redirected UTF-8 stdin.
# Never put the cookie in command-line arguments, shell history or an example file.
# bugboard-mcp.exe auth import --stdin --profile work

.\bugboard-mcp.exe auth status --profile work
.\bugboard-mcp.exe auth delete --profile work
```

`import` and `import-env` replace the chosen profile only after `/sys/auth/status`
returns `isAuthenticated: true`. Repeat import to refresh. Network errors, non-JSON
responses, false/missing auth status, cancelled/empty input or save errors fail without
replacing the previous record. The helper emits fixed error codes and safe JSON status,
never candidate cookies, raw HTTP error strings, response bodies or DPAPI errors.
`status` decrypts and checks live authentication. `delete` removes the local encrypted
record only: it does **not** revoke the remote browser/server session. Import/delete do
not modify an already-running MCP process; restart it after changing its profile.

## Explicit MCP selection and compatibility

Set the MCP environment to:

```toml
BUGBOARD_SESSION_STORE = "dpapi"
BUGBOARD_PROFILE = "work"
```

This explicitly selects protected storage **before** `BUGBOARD_COOKIE` or
`BUGBOARD_SESSION_ENV`. A missing/corrupt/unreadable/wrong-user protected record or
unknown store value fails closed; legacy credentials are never a fallback. Without
`BUGBOARD_SESSION_STORE`, the historical direct-cookie / env-file precedence remains.
No existing connection or legacy plaintext file is automatically changed or deleted.

Default store: `%LOCALAPPDATA%\bugboard-mcp\protected-sessions`. An absolute
`BUGBOARD_SESSION_ROOT` may select a separate dedicated directory outside Git. New or
empty directories can be initialized; an existing nonempty directory must contain the
exact Bugboard version marker. Filesystem roots, parent traversal, reparse points and
repositories are rejected before ACL changes. This prevents adopting an unrelated
AppData, home or shared directory. Profiles are 1–64 ASCII letters/digits/`_`/`-`,
case-insensitive and stored lowercase. Windows device names are rejected.

The directory/file DACL is protected and grants FullControl only to the current Windows
user and SYSTEM. Native DPAPI uses current-user protection and forbids prompts;
`CRYPTPROTECT_LOCAL_MACHINE` is never set. Copied blobs require the same Windows
identity and DPAPI keys. This protects data at rest, not from malware running as that
user or an administrator. Decrypted temporary byte buffers and local import copies
are cleared where owned; HTTP client/string allocations can still contain credentials
in process memory. Do not distribute process dumps.

Writes use a per-profile OS lock (released on process death), ciphertext-only temporary
file and atomic replacement. Delete shares the lock. Read sees one complete old/new
record. A crashed temporary ciphertext file is harmless and never reused automatically.
No cookie is saved in browser state, helper logs, command arguments or plaintext temp.
The PowerShell ACL helper receives only the path, no secret; its output is suppressed.

Each successful save generates a random 128-bit generation inside the encrypted record.
The MCP captures cookie and generation in **one** decrypted snapshot and passes that
generation to the product catalog cache. Replacing the account/session under the same
profile cannot select the old cache generation. Persistent cache is disabled for legacy
credentials that have no protected generation. Existing MCP processes retain their own
snapshot until restart; there is no shared mutable current account/project.

Windows is the supported protected-storage platform. Other systems explicitly return
`protected_sessions_unsupported_os`; plaintext fallback is not implicit. Windows
PowerShell and its .NET filesystem ACL APIs are required. Network auth verification
has the existing client timeout; local filesystem/ACL operations rely on Windows
completion and have no helper-specific timeout.

## Verification and remaining acceptance

Synthetic Windows tests exercise DPAPI roundtrip/integrity corruption, filename safety
and case aliases, profile-file swap rejection, refresh generation changes, deletion,
unrelated directory protection, rejected-import preservation and bounded/cancelled
input. ACL tests require an unrestricted current-user Windows process: sandboxed
tokens can be unable to assign the owner. These tests use only synthetic cookie values.

Live migration should import a valid existing session into an isolated profile, check
`auth status`, run MCP tools using explicit secure selection, verify corruption fails
closed even when legacy env remains set, then switch the working connection. Fresh
browser-login, expiry/re-login and another Windows user's inability to decrypt require
separate acceptance; unit tests must not be reported as proof of those user workflows.

Microsoft contracts checked during implementation:
[CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata),
[DirectorySecurity](https://learn.microsoft.com/en-us/dotnet/api/system.security.accesscontrol.directorysecurity),
[BCryptGenRandom](https://learn.microsoft.com/en-us/windows/win32/api/bcrypt/nf-bcrypt-bcryptgenrandom).
