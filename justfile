default: test

test:
    cargo test --locked

_install-llvm-cov:
    rustup component add llvm-tools-preview
    cargo install cargo-llvm-cov

cov: _install-llvm-cov
    cargo llvm-cov --locked --fail-under-lines 100

cov-html: _install-llvm-cov
    cargo llvm-cov --locked --fail-under-lines 100

build:
    cargo build --locked

release:
    cargo build --locked --release
