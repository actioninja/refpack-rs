fmt:
    cargo +nightly fmt

readme:
    cargo install cargo-rdme
    cargo rdme

long-tests:
    cargo test --include-ignored
