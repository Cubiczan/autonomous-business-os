.PHONY: install dev worker test lint docker mcp governance

install:
	python -m pip install -r requirements.txt
	python -m pip install -e ".[dev]"

dev:
	uvicorn app.main:app --reload --host 0.0.0.0 --port 8000

worker:
	python -m app.worker

test:
	pytest

lint:
	ruff check .

docker:
	docker compose up --build

governance:
	cargo build -p abos-governance-core

mcp: governance
	python -m app.mcp
