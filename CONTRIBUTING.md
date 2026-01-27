# Contributing to Fili

## Code Quality

Before committing, always run:

```bash
# Format code
cargo fmt

# Lint with warnings as errors
cargo clippy -- -D warnings
```

## Pre-commit Hook (optional)

Add to `.git/hooks/pre-commit`:

```bash
#!/bin/sh
cargo fmt -- --check || exit 1
cargo clippy -- -D warnings || exit 1
```

Then `chmod +x .git/hooks/pre-commit`.

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
