.PHONY: help build test run clean docker-up docker-down migrate

help:
	@echo "Whisper - Silent Payments Light Indexer"
	@echo ""
	@echo "Available commands:"
	@echo "  make build        - Build all crates"
	@echo "  make test         - Run all tests"
	@echo "  make run          - Run the server"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make docker-up    - Start Docker services"
	@echo "  make docker-down  - Stop Docker services"
	@echo "  make example      - Run client example"

build:
	cargo build --release

test:
	cargo test --all

run:
	cd whisper-server && cargo run --release

clean:
	cargo clean

docker-up:
	docker-compose up -d

docker-down:
	docker-compose down

docker-logs:
	docker-compose logs -f whisper-server

example:
	cd whisper-client && cargo run --example scan_example

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all -- -D warnings

check: fmt clippy test
	@echo "✓ All checks passed"
