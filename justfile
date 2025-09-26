check: format-check build lint test

build:
  cargo build --all-targets --all-features
  
format:
  cargo fmt --all
  ! command -v prettier > /dev/null || prettier --write .

format-check:
  cargo fmt --all --check
  ! command -v prettier > /dev/null || prettier --check .

lint:
  cargo clippy --all-targets --all-features
  cargo machete --with-metadata

lint-apply:
  cargo clippy --fix --all-targets --all-features

test:
  cargo test --all-targets --all-features
