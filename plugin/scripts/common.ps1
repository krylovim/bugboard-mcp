# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see ../LICENSE.
# Shared Windows lifecycle helpers. Never read or serialize session credentials.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-BugboardRoot([string]$InstallationRoot) {
    if ([string]::IsNullOrWhiteSpace($InstallationRoot)) {
        if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw 'local_app_data_missing' }
        $InstallationRoot = Join-Path $env:LOCALAPPDATA 'bugboard-mcp'
    }
    if (-not [IO.Path]::IsPathRooted($InstallationRoot)) { throw 'installation_root_must_be_absolute' }
    $resolved = [IO.Path]::GetFullPath($InstallationRoot)
    if ($resolved.TrimEnd('\','/') -eq [IO.Path]::GetPathRoot($resolved).TrimEnd('\','/')) { throw 'installation_root_not_dedicated' }
    $resolved = $resolved.TrimEnd('\','/')
    Assert-BugboardNoLinks $resolved
    $ancestor = $resolved
    while ($ancestor) {
        if (Test-Path -LiteralPath (Join-Path $ancestor '.git')) { throw 'installation_root_inside_repository' }
        $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    }
    return $resolved
}

function Assert-BugboardNoLinks([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'reparse_point_not_supported' }
        }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}

function Join-BugboardContained([string]$Root, [string]$Relative) {
    if ([string]::IsNullOrWhiteSpace($Relative) -or $Relative.Contains('\') -or $Relative.Contains(':') -or $Relative.StartsWith('/') -or $Relative.EndsWith('/')) { throw 'invalid_package_path' }
    foreach ($segment in $Relative.Split('/')) {
        if ($segment -in @('','.','..') -or $segment.EndsWith('.') -or $segment.EndsWith(' ') -or $segment.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0 -or $segment -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)') { throw 'invalid_package_path' }
    }
    $base = [IO.Path]::GetFullPath($Root).TrimEnd('\','/')
    $result = [IO.Path]::GetFullPath((Join-Path $base $Relative))
    if (-not $result.StartsWith($base + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'package_path_escapes_root' }
    Assert-BugboardNoLinks $result
    return $result
}

function Read-BugboardJson([string]$Path) {
    Assert-BugboardNoLinks $Path
    $file = Get-Item -LiteralPath $Path -Force
    if ($file.PSIsContainer -or $file.Length -gt 8MB) { throw 'invalid_json_file' }
    try { return ([IO.File]::ReadAllText($Path) | ConvertFrom-Json) } catch { throw 'invalid_json_file' }
}

function Get-BugboardField($Object, [string]$Name) {
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Get-BugboardProfile([string]$Profile) {
    if ($Profile -notmatch '^[A-Za-z0-9_-]{1,64}$' -or $Profile -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])$') { throw 'invalid_profile' }
    return $Profile.ToLowerInvariant()
}

function Get-BugboardSha256([string]$Path) {
    # Native hosts can pass a PowerShell 7 PSModulePath into Windows PowerShell
    # 5.1, where the Get-FileHash function then cannot autoload. Use the .NET
    # primitive directly so validation never depends on that host environment.
    $stream = [IO.File]::OpenRead($Path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}

function Assert-BugboardBundle([string]$PackageRoot, [string]$ExpectedManifestHash = '') {
    $package = [IO.Path]::GetFullPath($PackageRoot).TrimEnd('\','/')
    Assert-BugboardNoLinks $package
    $manifestPath = Join-BugboardContained $package 'bundle-manifest.json'
    $hash = Get-BugboardSha256 $manifestPath
    if ($ExpectedManifestHash -and $hash -cne $ExpectedManifestHash) { throw 'package_manifest_hash_mismatch' }
    $manifest = Read-BugboardJson $manifestPath
    $version = Get-BugboardField $manifest 'version'
    if ($version -isnot [string] -or $version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$' -or $version.EndsWith('.') -or $version -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)') { throw 'invalid_package_version' }
    foreach ($name in @('runtime_commit','skill_commit')) {
        if ((Get-BugboardField $manifest $name) -notmatch '^[a-fA-F0-9]{40}$') { throw 'invalid_package_commit' }
    }
    $files = Get-BugboardField $manifest 'files'
    if ($null -eq $files -or $files -isnot [PSCustomObject]) { throw 'invalid_package_inventory' }
    $inventory = [Collections.Generic.Dictionary[string,string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($property in $files.PSObject.Properties) {
        $relative = [string]$property.Name
        if ($relative -ieq 'bundle-manifest.json' -or $relative -match '(?i)(^|/)(\.git|\.aws|protected-sessions|backups|\.env[^/]*)(/|$)' -or $relative -match '(?i)(\.dpapi|\.env)$' -or $relative -ieq '.bugboard.json') { throw 'forbidden_package_file' }
        $destination = Join-BugboardContained $package $relative
        if ($property.Value -isnot [string] -or $property.Value -notmatch '^[a-fA-F0-9]{64}$' -or $inventory.ContainsKey($relative)) { throw 'invalid_package_inventory' }
        $inventory.Add($relative, $property.Value.ToLowerInvariant())
        $file = Get-Item -LiteralPath $destination -Force
        if ($file.PSIsContainer -or (Get-BugboardSha256 $destination) -cne $inventory[$relative]) { throw 'package_file_hash_mismatch' }
    }
    foreach ($required in @('runtime/bugboard-mcp.exe','auth/browser-login.cjs','scripts/common.ps1','scripts/install.ps1','scripts/launch.ps1','scripts/login.ps1','scripts/rollback.ps1','.codex-plugin/plugin.json','.mcp.json')) {
        if (-not $inventory.ContainsKey($required)) { throw 'package_required_file_missing' }
    }
    # Enumerate without following junctions; reject extras and case aliases.
    $directories = [Collections.Generic.Queue[string]]::new()
    $directories.Enqueue($package)
    $count = 0
    while ($directories.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $directories.Dequeue() -Force) {
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'reparse_point_not_supported' }
            if ($item.PSIsContainer) { $directories.Enqueue($item.FullName); continue }
            $relative = $item.FullName.Substring($package.Length + 1).Replace('\','/')
            if ($relative -ieq 'bundle-manifest.json') { continue }
            if (-not $inventory.ContainsKey($relative)) { throw 'unmanifested_package_file' }
            $count++
        }
    }
    if ($count -ne $inventory.Count) { throw 'package_inventory_mismatch' }
    return [PSCustomObject]@{Root=$package;Manifest=$manifest;Hash=$hash;Version=$version;Files=$inventory;ReleaseId=($version + '-' + $hash.Substring(0,12))}
}

function Protect-BugboardRoot([string]$Root) {
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner($sid)
    $acl.SetAccessRuleProtection($true,$false)
    foreach ($identity in @($sid,[Security.Principal.SecurityIdentifier]'S-1-5-18')) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))
    }
    # New descriptor contains only owner/DACL; do not request SACL privileges.
    if ($PSVersionTable.PSVersion.Major -ge 7) { Set-Acl -LiteralPath $Root -AclObject $acl }
    else { [IO.Directory]::SetAccessControl($Root,$acl) }
}

function Initialize-BugboardRoot([string]$Root) {
    $markerPath = Join-BugboardContained $Root '.bugboard-plugin-root'
    if (Test-Path -LiteralPath $Root) {
        if (Test-Path -LiteralPath $markerPath) {
            if ([IO.File]::ReadAllText($markerPath) -cne "bugboard-mcp plugin installation v1`n") { throw 'installation_root_not_dedicated' }
        } elseif (@(Get-ChildItem -LiteralPath $Root -Force).Count -gt 0) {
            $legacyPath = Join-BugboardContained $Root 'installation.json'
            if (-not (Test-Path -LiteralPath $legacyPath -PathType Leaf)) { throw 'installation_root_not_dedicated' }
            $legacy = Read-BugboardJson $legacyPath
            $legacyBinary = Get-BugboardField $legacy 'runtime'
            if ((Get-BugboardField $legacy 'version') -ne 2 -or $legacyBinary -isnot [string] -or (Get-BugboardField $legacy 'sha256') -notmatch '^[a-fA-F0-9]{64}$') { throw 'installation_root_not_dedicated' }
            $legacyFull = [IO.Path]::GetFullPath($legacyBinary)
            if (-not $legacyFull.StartsWith($Root + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'legacy_runtime_outside_root' }
            Assert-BugboardNoLinks $legacyFull
            if ((Get-BugboardSha256 $legacyFull) -ine $legacy.sha256) { throw 'legacy_runtime_hash_mismatch' }
        }
    } else { [void][IO.Directory]::CreateDirectory($Root) }
    Protect-BugboardRoot $Root
    if (-not (Test-Path -LiteralPath $markerPath)) { [IO.File]::WriteAllText($markerPath,"bugboard-mcp plugin installation v1`n",[Text.UTF8Encoding]::new($false)) }
    [void][IO.Directory]::CreateDirectory((Join-BugboardContained $Root 'backups'))
}

function Lock-BugboardInstallation([string]$Root) {
    $lockPath = Join-BugboardContained $Root '.plugin-installation.lock'
    try { return [IO.File]::Open($lockPath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None) }
    catch { throw 'plugin_installation_busy' }
}

function Read-BugboardState([string]$Root, [switch]$Optional) {
    $statePath = Join-BugboardContained $Root 'plugin-installation.json'
    if (-not (Test-Path -LiteralPath $statePath)) { if ($Optional) { return $null }; throw 'plugin_not_installed_run_install_script' }
    $state = Read-BugboardJson $statePath
    if ((Get-BugboardField $state 'schema') -ne 1 -or $null -eq (Get-BugboardField $state 'current')) { throw 'invalid_plugin_state' }
    return $state
}

function Resolve-BugboardRelease([string]$Root, $Entry) {
    $id = Get-BugboardField $Entry 'release_id'
    $hash = Get-BugboardField $Entry 'manifest_sha256'
    $version = Get-BugboardField $Entry 'version'
    if ($id -isnot [string] -or $id -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}-[a-f0-9]{12}$' -or $hash -notmatch '^[a-f0-9]{64}$') { throw 'invalid_release_reference' }
    $profile = Get-BugboardProfile (Get-BugboardField $Entry 'profile')
    $releasePath = Join-BugboardContained $Root ('plugin-releases/' + $id)
    $bundle = Assert-BugboardBundle $releasePath $hash
    if ($bundle.ReleaseId -cne $id -or $bundle.Version -cne $version) { throw 'release_reference_mismatch' }
    return [PSCustomObject]@{Root=$bundle.Root;Profile=$profile;Entry=$Entry;SkillCommit=$bundle.Manifest.skill_commit}
}

function Assert-BugboardRegisteredSkill([string]$PackageRoot, $Release) {
    # Codex loads the skill snapshot from its registered package, which can lag
    # an external runtime rollback/update. Never silently pair different skills.
    $registered = Read-BugboardJson (Join-BugboardContained $PackageRoot 'bundle-manifest.json')
    $skill = Get-BugboardField $registered 'skill_commit'
    if ($skill -isnot [string] -or $skill -notmatch '^[a-fA-F0-9]{40}$' -or $skill -ine $Release.SkillCommit) { throw 'plugin_skill_snapshot_mismatch_refresh_registration' }
}

function Write-BugboardState([string]$Root, $State) {
    $target = Join-BugboardContained $Root 'plugin-installation.json'
    $temporary = Join-BugboardContained $Root ('.plugin-state-' + [Guid]::NewGuid().ToString('N') + '.tmp')
    $backup = Join-BugboardContained $Root ('.plugin-state-' + [Guid]::NewGuid().ToString('N') + '.bak')
    $stream = [IO.File]::Open($temporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($State | ConvertTo-Json -Depth 8))
        $stream.Write($bytes,0,$bytes.Length)
        $stream.Flush($true)
    } finally { $stream.Dispose() }
    try {
        if (Test-Path -LiteralPath $target) { [IO.File]::Replace($temporary,$target,$backup) }
        else { [IO.File]::Move($temporary,$target) }
    } finally {
        # Only these exact newly-created state files, never recursive cleanup.
        foreach ($file in @($temporary,$backup)) { if (Test-Path -LiteralPath $file -PathType Leaf) { [IO.File]::Delete($file) } }
    }
}

function Set-BugboardLaunchEnvironment([string]$Profile, [string]$Root) {
    Remove-Item Env:BUGBOARD_COOKIE -ErrorAction SilentlyContinue
    Remove-Item Env:BUGBOARD_SESSION_ENV -ErrorAction SilentlyContinue
    $env:BUGBOARD_SESSION_STORE = 'dpapi'
    $env:BUGBOARD_PROFILE = Get-BugboardProfile $Profile
    $env:BUGBOARD_SESSION_ROOT = Join-BugboardContained $Root 'protected-sessions'
}
