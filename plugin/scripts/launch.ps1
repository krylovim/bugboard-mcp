# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see ../LICENSE.
param([string]$InstallationRoot = '')
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    $root = Get-BugboardRoot $InstallationRoot
    $state = Read-BugboardState $root
    $release = Resolve-BugboardRelease $root $state.current
    Assert-BugboardRegisteredSkill (Split-Path -Parent $PSScriptRoot) $release
    Set-BugboardLaunchEnvironment $release.Profile $root
    $binary = Join-BugboardContained $release.Root 'runtime/bugboard-mcp.exe'
    # stdout belongs exclusively to the MCP JSON-RPC transport.
    & $binary --stdio
    exit $LASTEXITCODE
} catch {
    if ($_.Exception.Message -ceq 'plugin_skill_snapshot_mismatch_refresh_registration') {
        [Console]::Error.WriteLine('Bugboard plugin skill snapshot differs from the active release. Refresh plugin registration and restart MCP before using this release.')
    } else { [Console]::Error.WriteLine('Bugboard plugin launch failed. Run install.ps1 and verify the active release; no legacy session was used.') }
    exit 1
}
