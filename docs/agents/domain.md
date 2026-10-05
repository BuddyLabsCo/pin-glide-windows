# Domain docs

## Layout and reading rules

This repository uses a single-context layout:

- GLOSSARY.md at the repository root.
- Architecture decision records under docs/adr/.

Before exploring the codebase, read GLOSSARY.md and ADRs relevant to the work.
If GLOSSARY-MAP.md exists, follow it to the glossaries relevant to the topic
and also check src/<context>/docs/adr/ for context-specific decisions.

If these files do not exist, proceed silently.
Create domain documents through domain-modeling when terms or decisions
actually get resolved.

## Vocabulary

Use glossary terms in issue titles, proposals, hypotheses, and test names.
Reconsider unfamiliar terms; note real vocabulary gaps for domain-modeling.

## Decisions

Explicitly surface any conflict with an existing ADR, identifying the ADR
and explaining why the decision may need revisiting.
