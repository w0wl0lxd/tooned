set shell := ["bash", "-uc"]

export CARGO_TERM_COLOR := "auto"

_default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --all-features --all-targets

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo nextest run --workspace --all-features --no-fail-fast

doc:
    RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --all-features

build:
    cargo build --all-features

build-release:
    cargo build --release --all-features

# Generate shell completions for all supported shells
completions:
    @mkdir -p target/completions
    cargo run -- completions --shell bash > target/completions/tooned.bash
    cargo run -- completions --shell zsh > target/completions/tooned.zsh
    cargo run -- completions --shell fish > target/completions/tooned.fish
    cargo run -- completions --shell nushell > target/completions/tooned.nu
    cargo run -- completions --shell elvish > target/completions/tooned.elv
    cargo run -- completions --shell powershell > target/completions/tooned.ps1
    @echo "Completions generated in target/completions/"

# Generate the man page
man:
    @mkdir -p target/man
    cargo run -- man > target/man/tooned.1
    @echo "Man page generated at target/man/tooned.1"

# Generate code coverage report
coverage:
    cargo llvm-cov nextest --workspace --all-features --lcov --output-path lcov.info
    @echo "Coverage report generated: lcov.info"

# Run benchmarks
bench:
    cargo bench --all-features

# cargo-rail helpers for monorepo plan/run/unify workflows
rail-plan:
    cargo rail plan --merge-base --explain

rail-run:
    cargo rail run --merge-base --profile ci

rail-build:
    cargo rail run --merge-base --surface build -- --all-features --all-targets

rail-test:
    cargo rail run --merge-base --surface test

rail-doc:
    cargo rail run --merge-base --surface docs -- --all-features

rail-unify-check:
    cargo rail unify --check

rail-unify:
    cargo rail unify

changelog-check:
    ./tools/check-changelog.sh

changelog-preview:
    @if command -v towncrier >/dev/null 2>&1; then \
        towncrier build --draft --version 0.0.0; \
    else \
        uvx --from towncrier towncrier build --draft --version 0.0.0; \
    fi

changelog-build version:
    @if command -v towncrier >/dev/null 2>&1; then \
        towncrier build --yes --version {{version}}; \
    else \
        uvx --from towncrier towncrier build --yes --version {{version}}; \
    fi

validate: fmt-check check clippy test rail-unify-check changelog-check
