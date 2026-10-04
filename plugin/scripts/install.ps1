# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see ../LICENSE.
param([string]$PackageRoot = (Split-Path -Parent $PSScriptRoot), [string]$InstallationRoot = '', [string]$Profile = '')
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    # Validate complete input before creating or changing installation state.
    $bundle = Assert-BugboardBundle $PackageRoot
    $root = Get-BugboardRoot $InstallationRoot
    Initialize-BugboardRoot $root
    $lock = Lock-BugboardInstallation $root
    try {
        $state = Read-BugboardState $root -Optional
        if ($state) { [void](Resolve-BugboardRelease $root $state.current) }
        $selectedProfile = if ($Profile) { Get-BugboardProfile $Profile } elseif ($state) { Get-BugboardProfile $state.current.profile } else { 'work' }
        $releasePath = Join-BugboardContained $root ('plugin-releases/' + $bundle.ReleaseId)
        if (Test-Path -LiteralPath $releasePath) { [void](Assert-BugboardBundle $releasePath $bundle.Hash) }
        else {
            $stage = Join-BugboardContained $root ('plugin-releases/.incomplete-' + [Guid]::NewGuid().ToString('N'))
            [void][IO.Directory]::CreateDirectory($stage)
            foreach ($relative in @($bundle.Files.Keys) + @('bundle-manifest.json')) {
                $source = Join-BugboardContained $bundle.Root $relative
                $destination = Join-BugboardContained $stage $relative
                [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination))
                [IO.File]::Copy($source,$destination,$false)
            }
            [void](Assert-BugboardBundle $stage $bundle.Hash)
            # Atomic publish, no overwrite of a partially installed or existing release.
            [IO.Directory]::Move($stage,$releasePath)
        }
        $current = [ordered]@{release_id=$bundle.ReleaseId;manifest_sha256=$bundle.Hash;version=$bundle.Version;profile=$selectedProfile}
        $previous = if ($state) { $state.current } else { $null }
        if ($state -and $state.current.release_id -ceq $bundle.ReleaseId -and $state.current.profile -ceq $selectedProfile) { $previous = Get-BugboardField $state 'previous' }
        $next = [ordered]@{schema=1;current=$current;previous=$previous;updated_at=[DateTime]::UtcNow.ToString('o')}
        Write-BugboardState $root $next
        [ordered]@{status='ready';version=$bundle.Version;profile=$selectedProfile;session_verified=$false;configuration_modified=$false;restart_mcp=$true} | ConvertTo-Json -Compress
    } finally { $lock.Dispose() }
} catch { [Console]::Error.WriteLine('Bugboard plugin installation failed. Package/state validation or filesystem access did not succeed.'); exit 1 }
