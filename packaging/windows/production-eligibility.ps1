$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'production-policy.ps1')

$event = Get-Content -LiteralPath $env:GITHUB_EVENT_PATH -Raw | ConvertFrom-Json
if (!(Test-ReleaseEvent $event $env:GITHUB_EVENT_NAME $env:GITHUB_REPOSITORY)) {
    [IO.File]::AppendAllText($env:GITHUB_OUTPUT, "eligible=false`n")
    return
}

$checkout = & git -C (Join-Path $PSScriptRoot '../..') rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the checked-out commit.' }
$commit = Get-AuthorizedReleaseCommit $event $env:GITHUB_EVENT_NAME $env:GITHUB_REPOSITORY $env:GITHUB_SHA $checkout
[IO.File]::AppendAllText($env:GITHUB_OUTPUT, "eligible=true`ncommit=$commit`n")
