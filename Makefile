PY_PROJECT := ralph_legacy
GO_PROJECT := ralph_tui
GO_CMD := $(GO_PROJECT)/cmd/ralph
GO_FILES := $(shell find $(GO_PROJECT) -name '*.go')
PY_TESTS := $(shell find $(PY_PROJECT) -name 'test_*.py' -o -name '*_test.py')

.PHONY: install update lint type-check format clean test generate build ci

install:
	uv sync --project $(PY_PROJECT) --extra dev
	cd $(GO_PROJECT) && go mod download

update:
	uv lock --project $(PY_PROJECT) --upgrade
	uv sync --project $(PY_PROJECT) --extra dev
	cd $(GO_PROJECT) && go get -u ./... && go mod tidy

lint:
	uv run --project $(PY_PROJECT) --extra dev ruff check --fix
	cd $(GO_PROJECT) && go vet ./...

type-check:
	uv run --project $(PY_PROJECT) --extra dev ty check
	cd $(GO_PROJECT) && go test ./... -run=^$$

format:
	uv run --project $(PY_PROJECT) --extra dev ruff format
	gofmt -w $(GO_FILES)

clean:
	rm -rf $(PY_PROJECT)/.venv $(PY_PROJECT)/.ruff_cache $(PY_PROJECT)/.pytest_cache $(PY_PROJECT)/.ty_cache
	find $(PY_PROJECT) -name '__pycache__' -type d -prune -exec rm -rf {} +
	find . -name '*.log' -type f -delete
	rm -f $(GO_PROJECT)/ralph
	cd $(GO_PROJECT) && go clean -cache -testcache

test:
	cd $(GO_PROJECT) && go test ./...
ifneq ($(strip $(PY_TESTS)),)
	uv run --project $(PY_PROJECT) --extra dev pytest
else
	@echo "No Python tests found under $(PY_PROJECT)."
endif

generate:
	@echo "No API type generation configured."

build:
	cd $(GO_PROJECT) && go build ./cmd/ralph

ci: generate format type-check lint build test
