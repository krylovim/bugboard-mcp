# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see ../LICENSE.
param([string]$InstallationRoot = '')
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    $root = Get-BugboardRoot $InstallationRoot
    $lock = Lock-BugboardInstallation $root
    try {
        $state = Read-BugboardState $root
        $previous = Get-BugboardField $state 'previous'
        if (-not $previous) { throw 'previous_release_missing' }
        [void](Resolve-BugboardRelease $root $previous)
        $returnRelease = $null
        try { [void](Resolve-BugboardRelease $root $state.current); $returnRelease = $state.current } catch { }
        # A corrupted active release must not prevent recovery to an intact one.
        Write-BugboardState $root ([ordered]@{schema=1;current=$previous;previous=$returnRelease;updated_at=[DateTime]::UtcNow.ToString('o')})
        [ordered]@{status='rolled_back';version=$previous.version;profile=$previous.profile;session_modified=$false;configuration_modified=$false;restart_mcp=$true} | ConvertTo-Json -Compress
    } finally { $lock.Dispose() }
} catch { [Console]::Error.WriteLine('Bugboard rollback failed. A verified previous release is required; no sessions or configuration were changed.'); exit 1 }
