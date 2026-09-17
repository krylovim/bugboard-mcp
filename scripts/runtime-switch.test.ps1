# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
$ErrorActionPreference='Stop'
$taskRoot=Join-Path ([IO.Path]::GetTempPath()) ('bugboard-switch-test-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $taskRoot 'backups') -Force | Out-Null
$taskOld=Join-Path $taskRoot 'previous.exe'
$taskNew=Join-Path $taskRoot 'current.exe'
[IO.File]::WriteAllText($taskOld,'fixture previous, never executed')
[IO.File]::WriteAllText($taskNew,'fixture current, never executed')
$taskConfig=Join-Path $taskRoot 'config.toml'
$taskInitial=@'
model = "keep-model"
[mcp_servers.bugboard]
command = "previous.exe"
args = ["--stdio"]
disabled_tools = ["bug_vote"]
[mcp_servers.bugboard.env]
BUGBOARD_SESSION_ENV = "legacy.env"
UNCHANGED = "keep"
[mcp_servers.other]
command = "other.exe"
'@
[IO.File]::WriteAllText($taskConfig,$taskInitial)
$taskManifest=@{
    version=2; configuration=$taskConfig; runtime=$taskNew; rollback_runtime=$taskOld
    sha256=(Get-FileHash -LiteralPath $taskNew).Hash
    rollback_sha256=(Get-FileHash -LiteralPath $taskOld).Hash
    auth_env=@{BUGBOARD_SESSION_STORE='dpapi';BUGBOARD_PROFILE='work'}
    rollback_auth_env=@{BUGBOARD_SESSION_ENV='legacy.env'}
}
$taskManifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $taskRoot 'installation.json') -Encoding utf8
$taskValidator=@'
import sys,tomllib
from pathlib import Path
d=tomllib.loads(Path(sys.argv[1]).read_text(encoding='utf-8'))
assert d['model']=='keep-model'
assert d['mcp_servers']['other']=={'command':'other.exe'}
b=d['mcp_servers']['bugboard']
assert b['command']==sys.argv[2]
assert b['args']==['--stdio'] and b['disabled_tools']==['bug_vote']
expected={'UNCHANGED':'keep'}
if sys.argv[3]=='Current':expected.update(BUGBOARD_SESSION_STORE='dpapi',BUGBOARD_PROFILE='work')
else:expected.update(BUGBOARD_SESSION_ENV='legacy.env')
assert b['env']==expected
'@
foreach($taskVersion in @('Current','Previous','Current')) {
    & (Join-Path $PSScriptRoot 'runtime-switch.ps1') -InstallationRoot $taskRoot -Version $taskVersion
    $taskBinary=if($taskVersion -eq 'Current'){$taskNew}else{$taskOld}
    $taskValidator | python - $taskConfig $taskBinary $taskVersion
    if($LASTEXITCODE -ne 0){throw 'Switch did not preserve parsed configuration'}
}
$taskBefore=[IO.File]::ReadAllText($taskConfig)
[IO.File]::WriteAllText($taskOld,'hash mismatch')
$taskRejected=$false
try { & (Join-Path $PSScriptRoot 'runtime-switch.ps1') -InstallationRoot $taskRoot -Version Previous } catch { $taskRejected=$true }
if(-not $taskRejected -or [IO.File]::ReadAllText($taskConfig) -cne $taskBefore){throw 'Hash mismatch changed configuration'}
'PASS: current/previous/current, unrelated settings, compatible authorization, hash rejection.'
# Contains only synthetic text; leave the unique temporary directory for inspection.
