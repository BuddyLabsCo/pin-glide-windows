function Test-ReleaseEvent($event, [string] $eventName, [string] $repository) {
    $pr = $event.pull_request
    return ($eventName -ceq 'pull_request' -and $event.action -ceq 'closed' -and
        $pr.merged -is [bool] -and $pr.merged -and
        $pr.head.ref -ceq 'auto-release' -and $pr.base.ref -ceq 'main' -and
        $repository -and $pr.head.repo.full_name -ceq $repository -and
        $pr.base.repo.full_name -ceq $repository)
}

function Get-AuthorizedReleaseCommit($event, [string] $eventName, [string] $repository,
    [string] $githubSha, [string] $checkout) {
    if (!(Test-ReleaseEvent $event $eventName $repository)) {
        throw 'Unauthorized release event.'
    }
    $commit = $event.pull_request.merge_commit_sha
    if ($commit -cnotmatch '\A[0-9a-f]{40}\z' -or $commit -cne $githubSha -or $commit -cne $checkout) {
        throw 'Merge SHA mismatch: the event, workflow SHA and checkout must agree.'
    }
    return $commit
}

function Get-ProductionSigningMode([string] $thumbprint, [string] $timestamp) {
    if (!$thumbprint -and !$timestamp) { return 'unsigned' }
    if (!$thumbprint -or !$timestamp) {
        throw 'Both production signing variables must be configured together.'
    }
    $uri = $null
    if ($thumbprint -notmatch '\A[0-9a-f]{40}\z' -or
        $timestamp -notmatch '\Ahttps?://[a-z0-9.-]+(?::[0-9]+)?(?:/[a-z0-9._/-]*)?\z' -or
        ![Uri]::TryCreate($timestamp, [UriKind]::Absolute, [ref] $uri) -or !$uri.Host) {
        throw 'Invalid signing configuration: check the certificate thumbprint and timestamp URL.'
    }
    return 'signed'
}
