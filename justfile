# List all available commands
default:
    @just --list

# Run all checks
check:
    #!/usr/bin/env bash
    failed=0
    just fmt || failed=1
    just lint || failed=1
    just test || failed=1
    exit $failed

# Check formatting
fmt:
    cargo fmt --check

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Run tests
test:
    cargo test
