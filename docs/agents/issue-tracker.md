# Issue tracker: GitHub

Issues and specs live in GitHub Issues for BuddyLabsCo/pin-glide-windows.
Use the gh CLI from this repository; it infers the repository from the remote.

## Conventions

- Create: gh issue create --title "..." --body-file <file>
- Read: gh issue view <number> --comments; fetch labels with --json labels.
- List: gh issue list --state open --json number,title,body,labels,comments
- Comment: gh issue comment <number> --body-file <file>
- Update body: gh issue edit <number> --body-file <file>
- Apply or remove labels: gh issue edit <number> --add-label "..." / --remove-label "..."
- Close: gh issue close <number> --comment "..."

For multiline content, save the exact text to a temporary file and use --body-file.
Match existing tracker content language. If no items exist, ask before the first write.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub issues and PRs share a number space. Resolve ambiguous numbers with
gh pr view <number>, falling back to gh issue view <number>.

## Skill operations

When a skill says "publish to the issue tracker", create a GitHub issue.
When it says "fetch the relevant ticket", run gh issue view <number> --comments.

## Wayfinding operations

- Map: one issue labelled wayfinder:map containing Notes, Decisions-so-far, and Fog.
- Child: link tickets as GitHub sub-issues. If unavailable, use a task list in the
  map and a "Part of #<map>" line in each child. Use wayfinder:<type> labels
  (research, prototype, grilling, task).
- Blocking: use native GitHub issue dependencies. API issue_id values are numeric
  database IDs, not issue numbers or node IDs. If unavailable, use a
  "Blocked by: #<number>" line. All blockers must be closed before work begins.
- Frontier: select the first open, unassigned child in map order with no open blockers.
- Claim: gh issue edit <number> --add-assignee @me
- Resolve: comment with the result, close the child, and append a concise result
  and link to the map's Decisions-so-far.
