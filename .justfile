set dotenv-load := false

# print options
default:
    @just --list --unsorted

# install cargo tools
init:
    cargo upgrade --incompatible
    cargo update
    cargo install cargo-readme

# generate README
readme:
    cargo readme > README.md

# format code
fmt:
    cargo fmt
    prettier --write .
    just --fmt --unstable

# check code
check:
    cargo check
    cargo clippy

# build project
build:
    cargo build --all-targets

# execute tests
test:
    cargo test run --all-targets

# execute benchmarks
bench:
    cargo bench

# execute benchmarks
example:
    cargo run --example main
