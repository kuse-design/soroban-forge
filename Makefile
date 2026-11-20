.PHONY: build test format lint audit hygiene clean doc release deploy-staging deploy-prod

SHELL := /bin/bash

build:
	cargo build --workspace --all-targets

build-release:
	cargo build --workspace --all-targets --release

test:
	cargo test --workspace --all-targets

test-contract:
	cargo test --workspace --package soroban-forge-escrow
	cargo test --workspace --package soroban-forge-vesting
	cargo test --workspace --package soroban-forge-multi-sig-wallet
	cargo test --workspace --package soroban-forge-dao-governance
	cargo test --workspace --package soroban-forge-subscription-payments
	cargo test --workspace --package soroban-forge-marketplace-royalties

format:
	cargo fmt --all

format-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

audit:
	cargo audit

hygiene:
	bash scripts/check-repo-hygiene.sh

doc:
	cargo doc --workspace --no-deps --open

clean:
	cargo clean
	rm -rf scripts/.generated

release: format lint audit test build-release
	@echo "Release checks passed. Deploy via GitHub Actions."

deploy-staging:
	@echo "Deploy contracts to Stellar Testnet"
	stellar contract deploy --wasm target/wasm32-unknown-unknown/release/*.wasm --source-account GD...

deploy-prod:
	@echo "Deploy contracts to Stellar Mainnet"
	stellar contract deploy --wasm target/wasm32-unknown-unknown/release/*.wasm --source-account GD...
