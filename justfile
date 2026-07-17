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

# cargo-rail helpers for monorepo plan/run/unify workflows
rail-plan:
    cargo rail plan --merge-base --explain

rail-run:
    cargo rail run --merge-base --profile ci

rail-build:
    cargo rail run --merge-base --surface build -- --all-features --all-targets

rail-test:
    cargo rail run --merge-base --surface test -- --all-features

rail-doc:
    cargo rail run --merge-base --surface docs -- --all-features

rail-unify-check:
    cargo rail unify --check

rail-unify:
    cargo rail unify

validate: fmt-check check clippy test rail-unify-check
