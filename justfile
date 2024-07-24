test:
    cargo clippy --all-targets -- -D warnings
    cargo clippy --all-targets --features=tor -- -D warnings
    cargo clippy --all-targets --no-default-features -- -D warnings
    cargo clippy --all-targets --no-default-features --features=tor -- -D warnings
    cargo build --examples
    cargo build --verbose --all
    cargo test --lib --verbose
    cargo build --examples --features=tor
    cargo build --verbose --all --features=tor
    cargo test --lib --verbose --features=tor
    cargo build --examples --no-default-features
    cargo build --verbose --all --no-default-features
    cargo test --lib --verbose --no-default-features
    cargo build --examples --no-default-features --features=tor
    cargo build --verbose --all --no-default-features --features=tor
    cargo test --lib --verbose --no-default-features --features=tor
