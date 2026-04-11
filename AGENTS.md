# AGENTS.md

Repository-level instructions for coding agents working in this repo.

## Git Hygiene

- Do not add or commit anything under `.planning/`.
- Treat `.planning/` as local workflow state only. It may exist on disk, but it must stay out of git.
- If `.planning/` files appear as tracked or staged, remove them from the index and keep the local files.

## Local-Only Files

- Do not add IDE, cache, or machine-specific generated files unless the user explicitly asks for them.
- Prefer keeping release and publish changes focused on source, docs, and required packaging metadata.
