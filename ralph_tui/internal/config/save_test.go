// Package config provides tests for config serialization and save logic.
// Entrypoint: go test ./...
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestSavePartialRepoRelativePaths(t *testing.T) {
	tmpDir := t.TempDir()
	repoRoot := filepath.Join(tmpDir, "repo")
	mustMkdirAll(t, repoRoot)

	dataDir := filepath.Join(repoRoot, "data")
	cacheDir := filepath.Join(repoRoot, ".ralph", "cache")
	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	logFile := filepath.Join(repoRoot, ".ralph", "cache", "ralph_tui.log")
	partial := PartialConfig{
		Paths: &PathsPartial{
			DataDir:  stringPtr(dataDir),
			CacheDir: stringPtr(cacheDir),
			PinDir:   stringPtr(pinDir),
		},
		Git:     &GitPartial{AutoCommit: boolPtr(true)},
		Loop:    &LoopPartial{SleepSeconds: intPtr(5)},
		Specs:   &SpecsPartial{AutofillScout: boolPtr(true)},
		UI:      &UIPartial{Theme: stringPtr("classic"), RefreshSeconds: intPtr(5)},
		Logging: &LoggingPartial{Level: stringPtr("info"), File: stringPtr(logFile)},
		Version: intPtr(1),
	}

	path := filepath.Join(repoRoot, ".ralph", "ralph.json")
	if err := SavePartial(path, partial, SaveOptions{RelativeRoot: repoRoot}); err != nil {
		t.Fatalf("SavePartial failed: %v", err)
	}

	payload := readJSONMap(t, path)
	pathsValue, ok := payload["paths"].(map[string]any)
	if !ok {
		t.Fatalf("expected paths map")
	}
	if got := pathsValue["data_dir"]; got != "data" {
		t.Fatalf("expected relative data_dir, got %#v", got)
	}
	if got := pathsValue["cache_dir"]; got != filepath.Join(".ralph", "cache") {
		t.Fatalf("expected relative cache_dir, got %#v", got)
	}
	if got := pathsValue["pin_dir"]; got != filepath.Join(".ralph", "pin") {
		t.Fatalf("expected relative pin_dir, got %#v", got)
	}

	loggingValue, ok := payload["logging"].(map[string]any)
	if !ok {
		t.Fatalf("expected logging map")
	}
	if got := loggingValue["file"]; got != filepath.Join(".ralph", "cache", "ralph_tui.log") {
		t.Fatalf("expected relative logging.file, got %#v", got)
	}
}

func TestSavePartialGlobalKeepsAbsolute(t *testing.T) {
	tmpDir := t.TempDir()
	homeDir := filepath.Join(tmpDir, "home")
	mustMkdirAll(t, homeDir)

	dataDir := filepath.Join(tmpDir, "external", "data")
	logFile := filepath.Join(tmpDir, "external", "logs", "ralph_tui.log")
	partial := PartialConfig{
		Paths: &PathsPartial{
			DataDir: stringPtr(dataDir),
			PinDir:  stringPtr(filepath.Join(homeDir, ".ralph", "pin")),
		},
		Git:     &GitPartial{AutoPush: boolPtr(true)},
		Loop:    &LoopPartial{SleepSeconds: intPtr(5)},
		Specs:   &SpecsPartial{AutofillScout: boolPtr(true)},
		UI:      &UIPartial{Theme: stringPtr("classic"), RefreshSeconds: intPtr(5)},
		Logging: &LoggingPartial{Level: stringPtr("info"), File: stringPtr(logFile)},
		Version: intPtr(1),
	}

	path := filepath.Join(homeDir, ".ralph", "ralph.json")
	if err := SavePartial(path, partial, SaveOptions{}); err != nil {
		t.Fatalf("SavePartial failed: %v", err)
	}

	payload := readJSONMap(t, path)
	pathsValue, ok := payload["paths"].(map[string]any)
	if !ok {
		t.Fatalf("expected paths map")
	}
	if got := pathsValue["data_dir"]; got != dataDir {
		t.Fatalf("expected absolute data_dir, got %#v", got)
	}

	loggingValue, ok := payload["logging"].(map[string]any)
	if !ok {
		t.Fatalf("expected logging map")
	}
	if got := loggingValue["file"]; got != logFile {
		t.Fatalf("expected absolute logging.file, got %#v", got)
	}
}

func readJSONMap(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal(data, &payload); err != nil {
		t.Fatalf("unmarshal json: %v", err)
	}
	return payload
}

func boolPtr(value bool) *bool {
	return &value
}
