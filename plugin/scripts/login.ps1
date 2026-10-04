# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see ../LICENSE.
param([string]$InstallationRoot = '', [string]$NodePath = '', [ValidateRange(30,1800)][int]$Timeout = 600)
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    $root = Get-BugboardRoot $InstallationRoot
    $state = Read-BugboardState $root
    $release = Resolve-BugboardRelease $root $state.current
    if (-not $NodePath) { $NodePath = (Get-Command node.exe -CommandType Application -ErrorAction Stop).Source }
    if (-not [IO.Path]::IsPathRooted($NodePath)) { throw 'node_path_must_be_absolute' }
    Assert-BugboardNoLinks $NodePath
    if (-not (Test-Path -LiteralPath $NodePath -PathType Leaf)) { throw 'node_not_available' }
    Remove-Item Env:NODE_OPTIONS -ErrorAction SilentlyContinue
    Remove-Item Env:NODE_PATH -ErrorAction SilentlyContinue
    $nodeVersion = & $NodePath --version
    if ($LASTEXITCODE -ne 0 -or $nodeVersion -notmatch '^v(\d+)\.' -or [int]$Matches[1] -lt 20) { throw 'node_20_required' }
    Set-BugboardLaunchEnvironment $release.Profile $root
    $helper = Join-BugboardContained $release.Root 'auth/browser-login.cjs'
    $binary = Join-BugboardContained $release.Root 'runtime/bugboard-mcp.exe'
    & $NodePath $helper --mcp $binary --profile $release.Profile --channel chrome --timeout $Timeout
    exit $LASTEXITCODE
} catch { [Console]::Error.WriteLine('Bugboard login could not start. Verify the plugin installation and Node.js >=20; run in an interactive terminal.'); exit 1 }
