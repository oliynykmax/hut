# hut — development helpers
#
#  make lint    — clippy + fmt check
#  make fmt     — auto-format
#  make test    — full test suite
#  make build   — release build

.PHONY: lint fmt test build fix

lint:
	@echo "→ clippy"
	@cargo clippy --all-targets -- -D warnings
	@echo "→ fmt check"
	@cargo fmt --check

fmt:
	cargo fmt

fix:
	@echo "→ clippy --fix"
	@cargo clippy --fix --allow-dirty --allow-staged
	@echo "→ fmt"
	@cargo fmt

test:
	cargo test

build:
	cargo build --release
