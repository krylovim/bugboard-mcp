# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
# Synthetic UTF-8 stdio probe under Windows PowerShell 5.1; no Bugboard session used.
$ErrorActionPreference = 'Stop'
$taskRoot = Join-Path ([IO.Path]::GetTempPath()) ('bugboard-stdio-test-' + [Guid]::NewGuid().ToString('N'))
$taskPackage = Join-Path $taskRoot 'package'
$taskInstall = Join-Path $taskRoot 'installation'
$taskScripts = Join-Path (Split-Path -Parent $PSScriptRoot) 'plugin/scripts'
foreach ($directory in @('scripts','runtime','auth','.codex-plugin')) { [void][IO.Directory]::CreateDirectory((Join-Path $taskPackage $directory)) }
foreach ($script in @('common.ps1','install.ps1','launch.ps1','login.ps1','rollback.ps1')) { [IO.File]::Copy((Join-Path $taskScripts $script),(Join-Path $taskPackage "scripts/$script")) }
foreach ($relative in @('auth/browser-login.cjs','.codex-plugin/plugin.json','.mcp.json')) { [IO.File]::WriteAllText((Join-Path $taskPackage $relative),'synthetic probe placeholder') }
$taskProbe = @'
using System;
using System.Text;
public class BugboardStdioProbe {
    public static int Main() {
        if (Environment.GetEnvironmentVariable("BUGBOARD_COOKIE") != null || Environment.GetEnvironmentVariable("BUGBOARD_SESSION_ENV") != null) return 91;
        if (Environment.GetEnvironmentVariable("BUGBOARD_SESSION_STORE") != "dpapi" || Environment.GetEnvironmentVariable("BUGBOARD_PROFILE") != "work") return 92;
        var output = Console.OpenStandardOutput();
        var prefix = Encoding.UTF8.GetBytes("{\"title\":\"\u041d\u0414\u0421 \u0411\u041f\"}\n");
        output.Write(prefix, 0, prefix.Length);
        Console.OpenStandardInput().CopyTo(output);
        return 37;
    }
}
'@
Add-Type -TypeDefinition $taskProbe -OutputAssembly (Join-Path $taskPackage 'runtime/bugboard-mcp.exe') -OutputType ConsoleApplication
$taskFiles = [ordered]@{}
foreach ($file in Get-ChildItem -LiteralPath $taskPackage -File -Recurse -Force) { $taskFiles[$file.FullName.Substring($taskPackage.Length+1).Replace('\','/')] = (Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant() }
[ordered]@{version='0.0.0-stdio-test';runtime_commit=('a'*40);skill_commit=('b'*40);files=$taskFiles} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskPackage 'bundle-manifest.json') -Encoding UTF8
$taskPowerShell = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
& $taskPowerShell -NoLogo -NoProfile -ExecutionPolicy Bypass -File (Join-Path $taskPackage 'scripts/install.ps1') -PackageRoot $taskPackage -InstallationRoot $taskInstall | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'synthetic_install_failed' }
$taskNodeProbe = @'
const {spawn,spawnSync}=require('node:child_process');
const assert=require('node:assert/strict');
const path=require('node:path');
const [ps, launcher, root]=process.argv.slice(2);
const env={...process.env,PSModulePath:'C:\\intentionally-missing-powershell-module-path',BUGBOARD_COOKIE:'synthetic-not-real',BUGBOARD_SESSION_ENV:'synthetic-not-real'};
// Reinstall through a native parent that does not normalize PSModulePath.
const install=spawnSync(ps,['-NoLogo','-NoProfile','-ExecutionPolicy','Bypass','-File',path.join(path.dirname(launcher),'install.ps1'),'-PackageRoot',path.dirname(path.dirname(launcher)),'-InstallationRoot',root],{env,windowsHide:true,encoding:'utf8'});
assert.equal(install.status,0,'installation must not depend on module autoload path');
const text=String.fromCodePoint(1053,1044,1057,32,1041,1055);
const input=Buffer.from(JSON.stringify({jsonrpc:'2.0',method:'synthetic',text:text+' '+String.fromCodePoint(0x1f600)})+'\n','utf8');
const prefix=Buffer.from(JSON.stringify({title:text})+'\n','utf8');
const expected=Buffer.concat([prefix, input]);
const child=spawn(ps,['-NoLogo','-NoProfile','-ExecutionPolicy','Bypass','-File',launcher,'-InstallationRoot',root],{stdio:['pipe','pipe','pipe'],windowsHide:true,env});
const chunks=[],errors=[];
let stage='waiting_initial_output_with_stdin_open';
child.stdout.on('data',x=>{
 chunks.push(x);const received=Buffer.concat(chunks);
 if(stage==='waiting_initial_output_with_stdin_open' && received.length>=prefix.length){
   assert.deepEqual(received.subarray(0,prefix.length),prefix);stage='waiting_response_with_stdin_open';child.stdin.write(input);
 }
 if(stage==='waiting_response_with_stdin_open' && received.length>=expected.length){
   assert.deepEqual(received,expected);stage='streaming_verified';child.stdin.end();
 }
});child.stderr.on('data',x=>errors.push(x));
const timeout=setTimeout(()=>{child.kill();process.stderr.write('synthetic_stdio_timeout:'+stage+'\n');process.exitCode=1;},20000);
child.once('close',code=>{clearTimeout(timeout);try{assert.equal(stage,'streaming_verified','output must arrive before stdin EOF');assert.equal(code,37,'child exit must be forwarded');assert.equal(Buffer.concat(errors).length,0,'stderr must be empty');assert.deepEqual(Buffer.concat(chunks),expected,'UTF8 bytes and JSON lines must be unchanged');process.stdout.write(JSON.stringify({status:'passed',module_path_independent:true,streaming_before_eof:true,utf8_roundtrip:true,exit_forwarded:true,legacy_env_cleared:true})+'\n');}catch(e){process.stderr.write('synthetic_stdio_assertion_failed: '+e.message+'\n');process.exitCode=1;}});
'@
$taskNodePath = Join-Path $taskRoot 'probe.cjs'
[IO.File]::WriteAllText($taskNodePath,$taskNodeProbe,[Text.UTF8Encoding]::new($false))
& node $taskNodePath $taskPowerShell (Join-Path $taskPackage 'scripts/launch.ps1') $taskInstall
if ($LASTEXITCODE -ne 0) { throw 'synthetic_stdio_failed' }
