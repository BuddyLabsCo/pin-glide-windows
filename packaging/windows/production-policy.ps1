function Get-AuthorizedReleaseCommit($event, [string] $eventName, [string] $repository,
    [string] $githubSha, [string] $checkout) {
    $pr = $event.pull_request
    if ($eventName -cne 'pull_request' -or $event.action -cne 'closed' -or
        $pr.merged -isnot [bool] -or !$pr.merged -or
        $pr.head.ref -cne 'auto-release' -or $pr.base.ref -cne 'main' -or
        !$repository -or $pr.head.repo.full_name -cne $repository -or
        $pr.base.repo.full_name -cne $repository) {
        throw 'Unauthorized release event.'
    }
    $commit = $pr.merge_commit_sha
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
