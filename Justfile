# List available recipes.
default:
    @just --list

# Run everything CI runs.
all: fmt check clippy test

# Check formatting.
fmt:
    cargo fmt --all --check

# Apply formatting.
fmt-fix:
    cargo fmt --all

# Type-check.
check:
    cargo check --all-targets

# Lint.
clippy:
    cargo clippy --all-targets -- -D warnings

# Run tests.
test:
    cargo test --all-targets

