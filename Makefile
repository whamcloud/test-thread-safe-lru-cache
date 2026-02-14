# Makefile for Rust project: code quality & security checks

# Default target
.PHONY: all
all: fmt clippy test audit build doc

# formatting
.PHONY: fmt
fmt:
	cargo fmt --all

# linting
.PHONY: clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# run UTs
.PHONY: test
test:
	cargo test

# build
.PHONY: build
build:
	cargo build

# release
.PHONY: release
release:
	cargo build --release

# doc
.PHONY: doc
doc:
	cargo doc --no-deps --document-private-items

# audit
.PHONY: audit
audit:
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit
	cargo audit

# scan unsafe
.PHONY: unsafe
unsafe:
	@command -v cargo-geiger >/dev/null 2>&1 || cargo install cargo-geiger --locked
	cargo geiger

# clean build
.PHONY: clean
clean:
	cargo clean

# benchmarking
.PHONY: bench
bench:
	cargo bench

# Run checks
.PHONY: full-check all
full-check: fmt clippy test build doc audit unsafe bench
