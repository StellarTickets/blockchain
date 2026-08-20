.PHONY: test build fmt lint

test:
	cargo test --workspace

build:
	stellar contract build

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings
