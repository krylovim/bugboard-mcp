# Copyright 2026 krylovim. Apache-2.0 + Commons Clause 1.0; see LICENSE.
# Install beside installation.json and auth-helper/browser-login.cjs.
param(
    [string]$Profile = 'work',
    [ValidateSet('chrome','msedge','chromium')][string]$Channel = 'chrome'
)
$ErrorActionPreference='Stop'
$taskManifest=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'installation.json') -Raw | ConvertFrom-Json
if((Get-FileHash -LiteralPath $taskManifest.runtime -Algorithm SHA256).Hash -ne $taskManifest.sha256){throw 'Runtime hash mismatch'}
$taskNode=(Get-Command node -ErrorAction Stop).Source
& $taskNode (Join-Path $PSScriptRoot 'auth-helper/browser-login.cjs') --mcp $taskManifest.runtime --profile $Profile --channel $Channel
exit $LASTEXITCODE
