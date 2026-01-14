// Package config provides tests for configuration loading and validation.
// Entrypoint: go test ./...
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/paths"
)

func TestLoadPrecedence(t *testing.T) {
	tmpDir := t.TempDir()
	homeDir := filepath.Join(tmpDir, "home")
	repoDir := filepath.Join(tmpDir, "repo")
	cwd := filepath.Join(repoDir, "work")

	mustMkdirAll(t, filepath.Join(homeDir, ".ralph"))
	mustMkdirAll(t, filepath.Join(repoDir, ".ralph"))
	mustMkdirAll(t, cwd)

	globalConfigPath := filepath.Join(homeDir, ".ralph", "ralph.json")
	repoConfigPath := filepath.Join(repoDir, ".ralph", "ralph.json")

	writeJSON(t, globalConfigPath, map[string]any{
		"logging": map[string]any{
			"level": "warn",
		},
		"ui": map[string]any{
			"theme": "global",
		},
		"paths": map[string]any{
			"data_dir": "global-data",
		},
	})

	writeJSON(t, repoConfigPath, map[string]any{
		"ui": map[string]any{
			"theme":           "repo",
			"refresh_seconds": 10,
		},
		"paths": map[string]any{
			"cache_dir": ".ralph/repo-cache",
		},
	})

	cliOverrides := PartialConfig{
		Logging: &LoggingPartial{Level: stringPtr("error")},
		Paths:   &PathsPartial{CacheDir: stringPtr("cli-cache")},
	}
	sessionOverrides := PartialConfig{
		UI: &UIPartial{Theme: stringPtr("session")},
	}

	cfg, err := LoadFromLocations(LoadOptions{
		Locations: paths.Locations{
			CWD:              cwd,
			RepoRoot:         repoDir,
			RepoConfigPath:   repoConfigPath,
			HomeDir:          homeDir,
			GlobalConfigPath: globalConfigPath,
		},
		SessionOverrides: sessionOverrides,
		CLIOverrides:     cliOverrides,
	})
	if err != nil {
		t.Fatalf("LoadFromLocations failed: %v", err)
	}

	if cfg.UI.Theme != "session" {
		t.Fatalf("expected session override theme, got %q", cfg.UI.Theme)
	}
	if cfg.UI.RefreshSeconds != 10 {
		t.Fatalf("expected repo refresh_seconds override, got %d", cfg.UI.RefreshSeconds)
	}
	if cfg.Logging.Level != "error" {
		t.Fatalf("expected cli override logging.level, got %q", cfg.Logging.Level)
	}

	expectedDataDir := filepath.Join(homeDir, "global-data")
	if cfg.Paths.DataDir != expectedDataDir {
		t.Fatalf("expected global data_dir %q, got %q", expectedDataDir, cfg.Paths.DataDir)
	}

	expectedCacheDir := filepath.Join(cwd, "cli-cache")
	if cfg.Paths.CacheDir != expectedCacheDir {
		t.Fatalf("expected cli cache_dir %q, got %q", expectedCacheDir, cfg.Paths.CacheDir)
	}
}

func TestLoadValidationFailure(t *testing.T) {
	tmpDir := t.TempDir()
	repoDir := filepath.Join(tmpDir, "repo")
	cwd := filepath.Join(repoDir, "work")

	mustMkdirAll(t, filepath.Join(repoDir, ".ralph"))
	mustMkdirAll(t, cwd)

	cfg, err := LoadFromLocations(LoadOptions{
		Locations: paths.Locations{
			CWD:            cwd,
			RepoRoot:       repoDir,
			RepoConfigPath: filepath.Join(repoDir, ".ralph", "ralph.json"),
		},
		SessionOverrides: PartialConfig{
			UI: &UIPartial{RefreshSeconds: intPtr(0)},
		},
	})
	if err == nil {
		t.Fatalf("expected validation error, got nil config: %+v", cfg)
	}
}

func writeJSON(t *testing.T, path string, payload any) {
	t.Helper()
	data, err := json.Marshal(payload)
	if err != nil {
		t.Fatalf("marshal json: %v", err)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatalf("write json: %v", err)
	}
}

func mustMkdirAll(t *testing.T, path string) {
	t.Helper()
	if err := os.MkdirAll(path, 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
}

func stringPtr(value string) *string {
	return &value
}

func intPtr(value int) *int {
	return &value
}
