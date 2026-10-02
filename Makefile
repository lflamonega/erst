.PHONY: build build-all test check lint coverage

build:
	cargo build --release

build-all:
	cargo build --release --target x86_64-unknown-linux-gnu --target aarch64-unknown-linux-gnu

test:
	cargo test

check: lint
	cargo fmt --check
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

coverage:
	cargo tarpaulin --out Html
