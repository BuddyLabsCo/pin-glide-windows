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
$head = & git -C $root rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Git checkout is unavailable.' }
$event.pull_request.merge_commit_sha = $head
$previous = @{}
$environment = @{
    GITHUB_EVENT_PATH = $fixture
    GITHUB_EVENT_NAME = 'pull_request'
    GITHUB_REPOSITORY = $repository
    GITHUB_SHA = $head
    PINGLIDE_SERVICE_URL = 'http://localhost'
    PINGLIDE_SIGNING_THUMBPRINT = ''
    PINGLIDE_TIMESTAMP_URL = ''
}
try {
    [IO.File]::WriteAllText($fixture, ($event | ConvertTo-Json -Depth 5))
    foreach ($name in $environment.Keys) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name)
        [Environment]::SetEnvironmentVariable($name, $environment[$name])
    }
    $candidate = Join-Path $PSScriptRoot 'production-candidate.ps1'
    $PSDefaultParameterValues = @{ '*:DevelopmentTransport' = $true }
    Assert-Rejected { & $candidate } 'Set PINGLIDE_SERVICE_URL'
    $env:PINGLIDE_SERVICE_URL = 'https://localhost'
    $env:PINGLIDE_SIGNING_THUMBPRINT = 'a' * 40
    Assert-Rejected { & $candidate } 'Both production signing variables'
    $env:PINGLIDE_TIMESTAMP_URL = 'https://timestamp.example.com'
    $missingSignTool = "missing-sign-tool-$([guid]::NewGuid()).exe"
    Assert-Rejected {
        & (Join-Path $PSScriptRoot 'build.ps1') -Unsigned:$false -DevelopmentTransport:$false -SignTool $missingSignTool
    } $missingSignTool
} finally {
    foreach ($name in $previous.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name])
    }
    Remove-Item -LiteralPath $fixture -ErrorAction SilentlyContinue
}

Write-Output 'Passed: production release authorization, exact checkout, signing policy, and fail-closed packaging.'
