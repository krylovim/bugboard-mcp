# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
# Installed beside installation.json. Switch executable and its compatible auth
# settings together; unrelated MCP configuration is preserved.
param(
    [ValidateSet('Current','Previous')][string]$Version = 'Previous',
    [string]$InstallationRoot = $PSScriptRoot
)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath($InstallationRoot)
$taskManifest = Get-Content -LiteralPath (Join-Path $taskRoot 'installation.json') -Raw | ConvertFrom-Json
if ($taskManifest.version -ne 2) { throw 'Unsupported runtime manifest version' }
if ($Version -eq 'Previous') {
    $taskBinary = $taskManifest.rollback_runtime
    $taskHash = $taskManifest.rollback_sha256
    $taskEnvironment = $taskManifest.rollback_auth_env
} else {
    $taskBinary = $taskManifest.runtime
    $taskHash = $taskManifest.sha256
    $taskEnvironment = $taskManifest.auth_env
}
if ((Get-FileHash -LiteralPath $taskBinary -Algorithm SHA256).Hash -ne $taskHash) { throw 'Runtime hash mismatch' }
$taskPath = $taskManifest.configuration
$taskText = [IO.File]::ReadAllText($taskPath)
$taskAuthKeys = @('BUGBOARD_SESSION_STORE','BUGBOARD_PROFILE','BUGBOARD_SESSION_ENV')
foreach ($taskProperty in $taskEnvironment.PSObject.Properties) {
    if ($taskProperty.Name -notin $taskAuthKeys -or $taskProperty.Value -isnot [string]) { throw 'Unexpected auth setting in runtime manifest' }
}
$taskSectionPattern = '(?ms)^\[mcp_servers\.bugboard\]\r?\n.*?(?=^\[|\z)'
$taskSections = [regex]::Matches($taskText, $taskSectionPattern)
if ($taskSections.Count -ne 1) { throw 'Expected one Bugboard section' }
$taskSection = $taskSections[0]
$taskCommandPattern = '(?m)^command\s*=.*$'
if ([regex]::Matches($taskSection.Value, $taskCommandPattern).Count -ne 1) { throw 'Expected one Bugboard command' }
$taskReplacement = 'command = ' + (ConvertTo-Json -InputObject ([string]$taskBinary) -Compress)
$taskChanged = [regex]::Replace($taskSection.Value, $taskCommandPattern, [Text.RegularExpressions.MatchEvaluator]{param($m) $taskReplacement})
$taskUpdated = $taskText.Substring(0,$taskSection.Index) + $taskChanged + $taskText.Substring($taskSection.Index+$taskSection.Length)
$taskEnvPattern = '(?ms)^\[mcp_servers\.bugboard\.env\]\r?\n.*?(?=^\[|\z)'
$taskEnvSections = [regex]::Matches($taskUpdated,$taskEnvPattern)
if ($taskEnvSections.Count -ne 1) { throw 'Expected one Bugboard environment section' }
$taskEnvSection = $taskEnvSections[0]
if ($taskEnvSection.Value -match '(?m)^BUGBOARD_COOKIE\s*=') { throw 'Direct credential setting needs separate migration' }
$taskEnvChanged = [regex]::Replace($taskEnvSection.Value, '(?m)^BUGBOARD_(SESSION_STORE|PROFILE|SESSION_ENV)\s*=.*\r?\n?', '')
$taskEnvChanged = $taskEnvChanged.TrimEnd() + "`n"
foreach ($taskProperty in $taskEnvironment.PSObject.Properties) {
    $taskEnvChanged += $taskProperty.Name + ' = ' + (ConvertTo-Json -InputObject ([string]$taskProperty.Value) -Compress) + "`n"
}
$taskEnvChanged += "`n"
$taskUpdated = $taskUpdated.Substring(0,$taskEnvSection.Index) + $taskEnvChanged + $taskUpdated.Substring($taskEnvSection.Index+$taskEnvSection.Length)
$taskStamp = [Guid]::NewGuid().ToString('N')
# InstallationRoot/backups inherits the restricted installation DACL. No config
# temporary copy is written in a public temporary directory.
$taskBackupDir = Join-Path $taskRoot 'backups'
if (-not (Test-Path -LiteralPath $taskBackupDir -PathType Container)) { throw 'Private backup directory missing' }
$taskTemp = Join-Path $taskBackupDir "config-switch-$taskStamp.tmp"
$taskBackup = Join-Path $taskBackupDir "config-before-switch-$taskStamp.toml"
$taskFile = [IO.File]::Open($taskTemp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $taskBytes = [Text.UTF8Encoding]::new($false).GetBytes($taskUpdated)
    $taskFile.Write($taskBytes,0,$taskBytes.Length)
    $taskFile.Flush($true)
} finally { $taskFile.Dispose() }
try {
    if ([IO.File]::ReadAllText($taskPath) -cne $taskText) { throw 'Configuration changed concurrently; no switch applied' }
    [IO.File]::Replace($taskTemp,$taskPath,$taskBackup)
} finally {
    if (Test-Path -LiteralPath $taskTemp) { Remove-Item -LiteralPath $taskTemp }
}
Write-Output "Configured $Version runtime and compatible authorization. Restart Bugboard MCP in open tasks."
