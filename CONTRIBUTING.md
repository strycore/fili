# Contributing to Fili

## Code Quality

Before committing, always run:

```bash
# Format code
cargo fmt

# Lint with warnings as errors
cargo clippy -- -D warnings
```

## Pre-commit Hook

A pre-commit hook lives in the repo under `.githooks/pre-commit`. It runs
`cargo fmt --check` and `cargo clippy -- -D warnings`, but only when
Rust sources or `Cargo.toml`/`Cargo.lock` are staged, so doc or
rules.json-only commits aren't slowed down.

Activate it once per clone:

```bash
git config core.hooksPath .githooks
```

This tells git to look in `.githooks/` for hooks instead of the usual
`.git/hooks/`. You can disable by unsetting: `git config --unset core.hooksPath`.

## Project Structure

```
src/
  main.rs      # CLI entry point
  db.rs        # SQLite database
  models.rs    # Data structures
  rules.rs     # Rule loading from JSON
  scanner.rs   # Filesystem scanning
rules.json     # Default skip patterns and contexts
```

## Adding New Rules

Edit `rules.json` to add skip patterns or collection contexts.
User overrides go in `~/.config/fili/rules.json`.
