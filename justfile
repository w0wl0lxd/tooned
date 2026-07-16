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

validate: fmt-check check clippy test
