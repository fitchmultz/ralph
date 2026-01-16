GO_PROJECT := ralph_tui
GO_CMD := $(GO_PROJECT)/cmd/ralph
GO_FILES := $(shell find $(GO_PROJECT) -name '*.go')
GO_TEST := go test -count=1

.PHONY: install update lint type-check format clean test generate pin-validate build ci

install:
	cd $(GO_PROJECT) && go mod download

update:
	cd $(GO_PROJECT) && go get -u ./... && go mod tidy

lint:
	cd $(GO_PROJECT) && go vet ./...

type-check:
	cd $(GO_PROJECT) && $(GO_TEST) ./... -run=^$$

format:
	gofmt -w $(GO_FILES)

clean:
	find . -name '*.log' -type f -delete
	rm -f $(GO_PROJECT)/ralph
	cd $(GO_PROJECT) && go clean -cache -testcache

test:
	cd $(GO_PROJECT) && $(GO_TEST) ./...

generate:
	@echo "No API type generation configured."

pin-validate:
	cd $(GO_PROJECT) && go run ./cmd/ralph pin validate

build:
	cd $(GO_PROJECT) && go build ./cmd/ralph

ci: generate format type-check lint pin-validate build test
