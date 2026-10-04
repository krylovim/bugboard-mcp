# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
# Synthetic fixtures only. Uses an isolated local temporary directory; never invokes a runtime.
param([string]$TestRoot = '')
$ErrorActionPreference = 'Stop'
$taskScripts = Join-Path (Split-Path -Parent $PSScriptRoot) 'plugin/scripts'
. (Join-Path $taskScripts 'common.ps1')
if (-not $TestRoot) { $TestRoot = Join-Path ([IO.Path]::GetTempPath()) ('bugboard-plugin-test-' + [Guid]::NewGuid().ToString('N')) }
$taskTestRoot = [IO.Path]::GetFullPath($TestRoot)
if (Test-Path -LiteralPath $taskTestRoot) { throw 'TestRoot must be new' }
[void][IO.Directory]::CreateDirectory($taskTestRoot)
$taskPowerShell = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'

function Assert-Test($Condition, [string]$Message) { if (-not $Condition) { throw "Test failed: $Message" } }
function New-Fixture([string]$Name, [string]$Version) {
    $root = Join-Path $taskTestRoot $Name
    [void][IO.Directory]::CreateDirectory($root)
    foreach ($script in @('common.ps1','install.ps1','launch.ps1','login.ps1','rollback.ps1')) {
        [void][IO.Directory]::CreateDirectory((Join-Path $root 'scripts'))
        [IO.File]::Copy((Join-Path $taskScripts $script),(Join-Path $root "scripts/$script"))
    }
    foreach ($relative in @('runtime/bugboard-mcp.exe','auth/browser-login.cjs','.codex-plugin/plugin.json','.mcp.json','skills/bugboard-diagnostics/SKILL.md')) {
        $path = Join-Path $root $relative
        [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
        [IO.File]::WriteAllText($path,"synthetic fixture $Version; never executable")
    }
    $files = [ordered]@{}
    foreach ($file in Get-ChildItem -LiteralPath $root -File -Recurse -Force) {
        $relative = $file.FullName.Substring($root.Length+1).Replace('\','/')
        $files[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    [ordered]@{version=$Version;runtime_commit=('a'*40);skill_commit=('b'*40);files=$files} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'bundle-manifest.json') -Encoding UTF8
    return $root
}
function Run-Script([string]$Name, [string[]]$Arguments, [bool]$Success) {
    $savedPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $taskOutput = & $taskPowerShell -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $taskScripts $Name) @Arguments 2>&1 }
    finally { $ErrorActionPreference = $savedPreference }
    $code = $LASTEXITCODE
    Assert-Test (($code -eq 0) -eq $Success) "$Name expected success=$Success; actual exit=$code"
    return ($taskOutput | Out-String)
}

$taskPackageOne = New-Fixture 'package-one' '0.1.0'
$taskPackageTwo = New-Fixture 'package-two' '0.2.0'
$taskInstallation = Join-Path $taskTestRoot 'installation'
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageOne,'-InstallationRoot',$taskInstallation,'-Profile','Work') $true)
$taskInitial = Read-BugboardState $taskInstallation
Assert-Test ($taskInitial.current.profile -ceq 'work') 'profile canonicalized'
Assert-Test ($null -eq $taskInitial.previous) 'fresh install no rollback'
$taskRootAcl = Get-Acl -LiteralPath $taskInstallation
Assert-Test $taskRootAcl.AreAccessRulesProtected 'installation DACL inheritance disabled'
$taskAllowedSids = @([Security.Principal.WindowsIdentity]::GetCurrent().User.Value,'S-1-5-18')
foreach ($rule in $taskRootAcl.Access) {
    Assert-Test ($rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -in $taskAllowedSids) 'installation DACL grants only current user and SYSTEM'
}
$taskFirstRelease = Resolve-BugboardRelease $taskInstallation $taskInitial.current
Assert-Test ($taskFirstRelease.Root.StartsWith($taskInstallation)) 'external immutable release selected'
# Synthetic protected data demonstrates no session/cache/binding cleanup on update/rollback.
foreach ($relative in @('protected-sessions/work.dpapi','catalog-cache/sentinel.json','.bugboard.json')) {
    $path = Join-Path $taskInstallation $relative
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
    [IO.File]::WriteAllText($path,'synthetic unchanged data, not a credential')
}
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageTwo,'-InstallationRoot',$taskInstallation) $true)
$taskUpdated = Read-BugboardState $taskInstallation
Assert-Test ($taskUpdated.current.version -ceq '0.2.0' -and $taskUpdated.current.profile -ceq 'work') 'update preserves profile'
Assert-Test ($taskUpdated.previous.release_id -ceq $taskInitial.current.release_id) 'update remembers previous release'
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageTwo,'-InstallationRoot',$taskInstallation) $true)
Assert-Test ((Read-BugboardState $taskInstallation).previous.release_id -ceq $taskInitial.current.release_id) 'idempotent install preserves rollback'
[void](Run-Script 'rollback.ps1' @('-InstallationRoot',$taskInstallation) $true)
Assert-Test ((Read-BugboardState $taskInstallation).current.release_id -ceq $taskInitial.current.release_id) 'rollback selects previous'
foreach ($relative in @('protected-sessions/work.dpapi','catalog-cache/sentinel.json','.bugboard.json')) {
    Assert-Test ([IO.File]::ReadAllText((Join-Path $taskInstallation $relative)) -ceq 'synthetic unchanged data, not a credential') 'protected data preserved'
}
# Concurrent writer fails without replacing state and an existing lock file is reusable.
$taskBeforeLock = [IO.File]::ReadAllText((Join-Path $taskInstallation 'plugin-installation.json'))
$taskLock = Lock-BugboardInstallation $taskInstallation
try { [void](Run-Script 'rollback.ps1' @('-InstallationRoot',$taskInstallation) $false) } finally { $taskLock.Dispose() }
Assert-Test ([IO.File]::ReadAllText((Join-Path $taskInstallation 'plugin-installation.json')) -ceq $taskBeforeLock) 'busy lock preserves state'
[void](Run-Script 'rollback.ps1' @('-InstallationRoot',$taskInstallation) $true)
# Source hash tamper cannot touch installation state.
[IO.File]::AppendAllText((Join-Path $taskPackageOne 'runtime/bugboard-mcp.exe'),'tampered')
$taskBeforeTamper = [IO.File]::ReadAllText((Join-Path $taskInstallation 'plugin-installation.json'))
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageOne,'-InstallationRoot',$taskInstallation) $false)
Assert-Test ([IO.File]::ReadAllText((Join-Path $taskInstallation 'plugin-installation.json')) -ceq $taskBeforeTamper) 'hash rejection preserves active state'
# Exact inventory rejects unexpected files, including credential-shaped files.
[IO.File]::WriteAllText((Join-Path $taskPackageTwo 'unexpected.txt'),'extra')
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageTwo,'-InstallationRoot',(Join-Path $taskTestRoot 'not-created')) $false)
Assert-Test (-not (Test-Path -LiteralPath (Join-Path $taskTestRoot 'not-created'))) 'invalid package checked before root creation'
[IO.File]::Delete((Join-Path $taskPackageTwo 'unexpected.txt'))
$taskUnrelated = Join-Path $taskTestRoot 'unrelated'
[void][IO.Directory]::CreateDirectory($taskUnrelated)
[IO.File]::WriteAllText((Join-Path $taskUnrelated 'unrelated.txt'),'unchanged')
$taskAclBefore = (Get-Acl -LiteralPath $taskUnrelated).Sddl
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageTwo,'-InstallationRoot',$taskUnrelated) $false)
Assert-Test ((Get-Acl -LiteralPath $taskUnrelated).Sddl -ceq $taskAclBefore) 'unrelated root ACL untouched'
# Corruption of active v2 must not block rollback to intact v1.
$taskCorruptState = Read-BugboardState $taskInstallation
$taskActive = Resolve-BugboardRelease $taskInstallation $taskCorruptState.current
[IO.File]::AppendAllText((Join-Path $taskActive.Root 'runtime/bugboard-mcp.exe'),'corrupt active')
[void](Run-Script 'rollback.ps1' @('-InstallationRoot',$taskInstallation) $true)
$taskRecovered = Read-BugboardState $taskInstallation
Assert-Test ($taskRecovered.current.version -ceq '0.1.0' -and $null -eq $taskRecovered.previous) 'rollback from corrupted release discards bad return target'
$taskRegistered = Read-BugboardJson (Join-Path $taskPackageTwo 'bundle-manifest.json')
$taskRegistered.skill_commit = 'c' * 40
$taskRegistered | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskPackageTwo 'bundle-manifest.json') -Encoding UTF8
$taskMismatchBlocked = $false
try { Assert-BugboardRegisteredSkill $taskPackageTwo (Resolve-BugboardRelease $taskInstallation $taskRecovered.current) } catch { $taskMismatchBlocked = $_.Exception.Message -ceq 'plugin_skill_snapshot_mismatch_refresh_registration' }
Assert-Test $taskMismatchBlocked 'mismatched registered skill cannot launch active runtime'
foreach ($invalid in @('../escape','C:/escape','/escape','a\b','a/../b','a/NUL.txt','a/file:stream','a/trailing.')) {
    $rejected = $false
    try { [void](Join-BugboardContained $taskTestRoot $invalid) } catch { $rejected = $true }
    Assert-Test $rejected 'invalid relative path rejected'
}
# Existing v2 deployment may be adopted without touching its legacy manifest/runtime.
$taskLegacy = Join-Path $taskTestRoot 'legacy'
[void][IO.Directory]::CreateDirectory($taskLegacy)
$taskLegacyExe = Join-Path $taskLegacy 'old.exe'
[IO.File]::WriteAllText($taskLegacyExe,'synthetic old binary, not executable')
$taskLegacyText = [ordered]@{version=2;runtime=$taskLegacyExe;sha256=(Get-FileHash -LiteralPath $taskLegacyExe).Hash} | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $taskLegacy 'installation.json'),$taskLegacyText)
[void](Run-Script 'install.ps1' @('-PackageRoot',$taskPackageTwo,'-InstallationRoot',$taskLegacy) $true)
Assert-Test ([IO.File]::ReadAllText((Join-Path $taskLegacy 'installation.json')) -ceq $taskLegacyText) 'legacy manifest unchanged'
Assert-Test ([IO.File]::ReadAllText($taskLegacyExe) -ceq 'synthetic old binary, not executable') 'legacy binary unchanged'
# No recursive deletion: the printed directory is an isolated reproducible test artifact.
[ordered]@{status='passed';groups=10;test_root=$taskTestRoot;real_installation_modified=$false;real_credentials_used=$false} | ConvertTo-Json -Compress
