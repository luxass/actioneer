set positional-arguments

default:
    @just --list

build:
    cargo build

test:
    cargo test

fmt:
    cargo fmt --all

run *args:
    cargo run -- "$@"

validate *args:
    cargo run -- validate "$@"

zizmor *args:
    zizmor "$@"

verbose *args:
    VERBOSE=true cargo run -- "$@"

clean:
    rm -rf target .zig-cache zig-out zig-pkg
