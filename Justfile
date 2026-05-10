set positional-arguments

default:
    @just --list

build:
    zig build

test:
    zig build test

fmt:
    zig fmt build.zig src

run *args:
    zig build run -- "$@"

validate *args:
    zig build run -- validate "$@"

verbose *args:
    VERBOSE=true zig build run -- "$@"

clean:
    rm -rf .zig-cache zig-out
