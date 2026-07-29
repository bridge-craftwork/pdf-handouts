# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

<!-- TODO: Add project description -->

## Build & Test Commands

```bash
cargo build                    # Build debug
cargo build --release          # Build release
cargo test                     # Run all tests
cargo clippy -- -D warnings    # Lint (treat warnings as errors)
cargo fmt --check              # Check formatting
```

## Pre-commit Requirements

Before committing, always run and fix:
1. `cargo fmt --all` - Format all code
2. `cargo clippy --all-targets -- -D warnings` - Fix all clippy warnings
3. `cargo test` - Ensure all tests pass

## Code Standards

- No `unwrap()` or `expect()` outside test code - use proper error handling
- No `println!()` in library code (CLI binaries are OK)
- All public functions must have doc comments (`///`)
- All `unsafe` blocks must have a comment explaining why they're safe
- Prefer editing existing files over creating new ones

## Git Configuration

Use SSH for all GitHub operations:
- Clone/push/pull: `git@github.com:Rick-Wilson/repo.git` (not `https://`)
- Remote URLs should use SSH format

## Related Projects

All located at `/Users/rick/Development/GitHub/`:

| Project | Description | Relationship |
|---------|-------------|--------------|
| [pbn-to-pdf](../pbn-to-pdf) | PDF generation | sibling |
| [printpdf-fork](../printpdf-fork) | PDF library fork | upstream dependency |

### Before running cargo in a sibling repo

**Read that repo's own `CLAUDE.md` first, and check for a `dev-build.sh`.** Most
of the bridge repos have one, and in those, bare `cargo build`/`test`/`clippy`
does the wrong thing: they carry gitignored `[patch]` overrides in
`.cargo/config.toml` pointing sibling crates at local checkouts, so cargo either
silently rewrites `Cargo.lock` with local paths that must never be committed, or
silently ignores the patches and builds the GitHub revisions instead of your
edits. Use `./dev-build.sh <subcommand>`, or `--ci` for CI parity, and confirm
`git status Cargo.lock` is clean before committing.

Have the script, so require it (11): `bridge-solver`, `bridge-solver-service`,
`bridge-table-service`, `bridge-wrangler`, `bridge-encodings`, `bridge-rulebot`,
`Bridge-Parsers`, `Bridge-Event-Parser-Service`, `EDGAR-Defense-Toolkit`,
`dealer3`, `pbn-to-pdf`.

No internal git dependencies, so bare cargo is safe (4): this repo,
`bridge-types`, `printpdf-fork`, `Bridge-Dealer-Service`.

## Notifications

Send Pushover notifications when work is blocked or completed:

```bash
pushover "message" "title"    # title defaults to "Claude Code"
```

**When to notify:**
- Waiting for user input or permission
- Task completed after extended work
- Build/test failures that need attention
- Any situation where work is paused and user may not notice
