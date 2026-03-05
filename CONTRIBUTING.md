# Contributing to TunnelMux

## Development Setup

```bash
cargo check
cargo test
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

## Pull Request Checklist

- Keep changes scoped and reviewable.
- Add/adjust tests for behavior changes.
- Update docs when APIs or CLI flags change.
- Ensure CI passes.

## Commit Style

Recommended conventional prefixes:

- `feat:`
- `fix:`
- `docs:`
- `chore:`
- `test:`

## Reporting Issues

When reporting a bug, include:

- TunnelMux version / commit SHA
- OS and architecture
- relevant command and logs
- expected behavior vs actual behavior
