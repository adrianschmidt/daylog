# daylog development commands

default:
    @just --list

# Build in debug mode
build:
    cargo build

# Build release binary
release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Run clippy + fmt check
lint:
    cargo fmt -- --check
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Run the TUI
run:
    cargo run

# Run with demo data (init first if needed)
demo:
    cargo run -- init --no-demo 2>/dev/null || true
    cargo run

# Initialize with default settings
init:
    cargo run -- init

# Sync notes to database
sync:
    cargo run -- sync

# Edit today's note
edit:
    cargo run -- edit

# Rebuild database from all notes
rebuild:
    cargo run -- rebuild

# Print today's status as JSON
status:
    cargo run -- status

# Log a value (usage: just log weight 173.4)
log *ARGS:
    cargo run -- log {{ARGS}}

# Generate shell completions
completions SHELL="fish":
    cargo run -- completions {{SHELL}}

# Install locally
install:
    cargo install --path .

# Check binary size
size:
    @cargo build --release 2>/dev/null
    @ls -lh target/release/daylog | awk '{print $5}'
