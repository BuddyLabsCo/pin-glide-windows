$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'production-policy.ps1')

function Assert-Rejected([scriptblock] $action, [string] $message) {
    try {
        & $action | Out-Null
    } catch {
        if (!$_.Exception.Message.Contains($message)) { throw }
        return
    }
    throw "Expected rejection: $message"
}

$commit = '0123456789abcdef0123456789abcdef01234567'
$repository = 'BuddyLabsCo/pin-glide-windows'
$event = @{
    action = 'closed'
    pull_request = @{
        merged = $true
        merge_commit_sha = $commit
        head = @{ ref = 'auto-release'; repo = @{ full_name = $repository } }
        base = @{ ref = 'main'; repo = @{ full_name = $repository } }
    }
}

function Authorize($event, $checkout = $commit, $githubSha = $commit, $eventName = 'pull_request') {
    Get-AuthorizedReleaseCommit $event $eventName $repository $githubSha $checkout
}

if ((Authorize $event) -ne $commit) { throw 'Authorized merge SHA was not retained.' }
$event.pull_request.merged = $false
Assert-Rejected { Authorize $event } 'Unauthorized release event'
$event.pull_request.merged = $true
$event.pull_request.head.ref = 'ordinary-change'
Assert-Rejected { Authorize $event } 'Unauthorized release event'
$event.pull_request.head.ref = 'auto-release'
$event.pull_request.head.repo.full_name = 'fork/pin-glide-windows'
Assert-Rejected { Authorize $event } 'Unauthorized release event'
$event.pull_request.head.repo.full_name = $repository
$event.pull_request.base.ref = 'other'
Assert-Rejected { Authorize $event } 'Unauthorized release event'
$event.pull_request.base.ref = 'main'
Assert-Rejected { Authorize $event -eventName 'workflow_dispatch' } 'Unauthorized release event'
Assert-Rejected { Authorize $event -checkout ('a' * 40) } 'Merge SHA mismatch'
Assert-Rejected { Authorize $event -githubSha ('b' * 40) } 'Merge SHA mismatch'

if ((Get-ProductionSigningMode '' '') -ne 'unsigned') { throw 'Expected unsigned packaging.' }
if ((Get-ProductionSigningMode ('a' * 40) 'https://timestamp.example.com') -ne 'signed') {
    throw 'Expected signed packaging.'
}
Assert-Rejected { Get-ProductionSigningMode ('a' * 40) '' } 'Both production signing variables'
Assert-Rejected { Get-ProductionSigningMode '' 'https://timestamp.example.com' } 'Both production signing variables'
Assert-Rejected { Get-ProductionSigningMode ' ' 'https://timestamp.example.com' } 'Invalid signing configuration'
Assert-Rejected { Get-ProductionSigningMode (('a' * 40) + "`n") 'https://timestamp.example.com' } 'Invalid signing configuration'
Assert-Rejected { Get-ProductionSigningMode ('a' * 40) 'not-a-url' } 'Invalid signing configuration'
Assert-Rejected { Get-ProductionSigningMode ('a' * 40) 'https://timestamp.example.com:99999' } 'Invalid signing configuration'

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$target = (New-Item -ItemType Directory -Force (Join-Path $root 'target')).FullName
$fixture = Join-Path $target "release-event-$([guid]::NewGuid()).json"
$workflowOutput = "$fixture.output"
$head = & git -C $root rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Git checkout is unavailable.' }
$event.pull_request.merge_commit_sha = $head
$previous = @{}
$environment = @{
    GITHUB_EVENT_PATH = $fixture
    GITHUB_EVENT_NAME = 'pull_request'
    GITHUB_REPOSITORY = $repository
    GITHUB_SHA = $head
    GITHUB_OUTPUT = $workflowOutput
    PINGLIDE_SERVICE_URL = 'http://localhost'
    PINGLIDE_SIGNING_THUMBPRINT = ''
    PINGLIDE_TIMESTAMP_URL = ''
}
try {
    foreach ($name in $environment.Keys) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name)
        [Environment]::SetEnvironmentVariable($name, $environment[$name])
    }
    $eligibility = Join-Path $PSScriptRoot 'production-eligibility.ps1'
    $event.pull_request.head.ref = 'Auto-Release'
    [IO.File]::WriteAllText($fixture, ($event | ConvertTo-Json -Depth 5))
    [IO.File]::WriteAllText($workflowOutput, '')
    & $eligibility
    if ((Get-Content -LiteralPath $workflowOutput -Raw).Trim() -cne 'eligible=false') {
        throw 'A case-variant release branch must skip the production job.'
    }
    $event.pull_request.head.ref = 'auto-release'
    [IO.File]::WriteAllText($fixture, ($event | ConvertTo-Json -Depth 5))
    [IO.File]::WriteAllText($workflowOutput, '')
    & $eligibility
    $outputs = Get-Content -LiteralPath $workflowOutput
    if ($outputs -cnotcontains 'eligible=true' -or $outputs -cnotcontains "commit=$head") {
        throw 'The exact release branch must authorize the merge commit.'
    }
    [IO.File]::WriteAllText($workflowOutput, '')
    $candidate = Join-Path $PSScriptRoot 'production-candidate.ps1'
    $PSDefaultParameterValues = @{ '*:DevelopmentTransport' = $true }
    Assert-Rejected { & $candidate } 'Set PINGLIDE_SERVICE_URL'
    $env:PINGLIDE_SERVICE_URL = 'https://localhost'
    $env:PINGLIDE_SIGNING_THUMBPRINT = 'a' * 40
    Assert-Rejected { & $candidate } 'Both production signing variables'
    $env:PINGLIDE_TIMESTAMP_URL = 'https://timestamp.example.com'
    $missingSignTool = "missing-sign-tool-$([guid]::NewGuid()).exe"
    $PSDefaultParameterValues['*:SignTool'] = $missingSignTool
    $artifacts = @('Pin-Glide-Setup-x64.exe', 'Pin-Glide-Setup-x64-unsigned.exe', 'release-candidate.json') |
        ForEach-Object { Join-Path $target "packaging/windows/$_" }
    $before = @{}
    foreach ($artifact in $artifacts) {
        $before[$artifact] = if (Test-Path -LiteralPath $artifact) {
            (Get-FileHash -LiteralPath $artifact).Hash
        }
    }
    Assert-Rejected { & $candidate } $missingSignTool
    foreach ($artifact in $artifacts) {
        $after = if (Test-Path -LiteralPath $artifact) { (Get-FileHash -LiteralPath $artifact).Hash }
        if ($after -ne $before[$artifact]) { throw "Signing failure changed $artifact." }
    }
    if ((Get-Item -LiteralPath $workflowOutput).Length -ne 0) {
        throw 'Signing failure emitted successful artifact output.'
    }
} finally {
    foreach ($name in $previous.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name])
    }
    Remove-Item -LiteralPath $fixture, $workflowOutput -ErrorAction SilentlyContinue
}

Write-Output 'Passed: production release authorization, exact checkout, signing policy, and fail-closed packaging.'
