PYTHON ?= python
.PHONY: install run test lint check docker-up docker-down

install:
	$(PYTHON) -m pip install -c requirements.lock -e '.[dev]'

run:
	$(PYTHON) -m whisper

test:
	$(PYTHON) -m pytest

lint:
	$(PYTHON) -m ruff check whisper tests examples
	$(PYTHON) -m ruff format --check whisper tests examples

check: lint test

docker-up:
	docker compose up --build -d

docker-down:
	docker compose down
