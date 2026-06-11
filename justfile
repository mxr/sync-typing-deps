default: test

test:
    cargo test --locked

_install-llvm-cov:
    rustup component add llvm-tools-preview
    cargo install cargo-llvm-cov

cov: _install-llvm-cov
    cargo llvm-cov --locked --fail-under-lines 100 --ignore-filename-regex 'src/main\.rs'

cov-html: _install-llvm-cov
    cargo llvm-cov --locked --open --ignore-filename-regex 'src/main\.rs'

lint:
    cargo clippy -- -D warnings

fmt:
    cargo fmt

build:
    cargo build --locked

release:
    cargo build --locked --release
