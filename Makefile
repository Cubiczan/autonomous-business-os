.PHONY: install dev worker test lint docker

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
