// Package config provides tests for configuration loading and validation.
// Entrypoint: go test ./...
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
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

func TestLoggingFileResolvesRelative(t *testing.T) {
	tmpDir := t.TempDir()
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}
	base = ResolvePaths(base, tmpDir)

	partial := PartialConfig{
		Logging: &LoggingPartial{File: stringPtr("logs/ralph_tui.log")},
	}
	cfg, err := ApplyPartial(base, partial, tmpDir)
	if err != nil {
		t.Fatalf("ApplyPartial failed: %v", err)
	}

	expected := filepath.Join(tmpDir, "logs", "ralph_tui.log")
	if cfg.Logging.File != expected {
		t.Fatalf("expected logging.file to resolve to %q, got %q", expected, cfg.Logging.File)
	}
	if err := cfg.Validate(); err != nil {
		t.Fatalf("expected config to validate, got %v", err)
	}
}

func TestApplyPartialNormalizesRunnerSettings(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	partial := PartialConfig{
		Specs: &SpecsPartial{
			Runner:          stringPtr("codex"),
			RunnerArgs:      []string{"  -c", "model_reasoning_effort=\"high\" ", " ", ""},
			ReasoningEffort: stringPtr(" High "),
		},
		Loop: &LoopPartial{
			Runner:          stringPtr("opencode"),
			RunnerArgs:      []string{" --flag", "value ", "", "  "},
			ReasoningEffort: stringPtr(" AUTO "),
		},
	}

	cfg, err := ApplyPartial(base, partial, ".")
	if err != nil {
		t.Fatalf("ApplyPartial failed: %v", err)
	}
	if got := cfg.Specs.ReasoningEffort; got != "high" {
		t.Fatalf("expected specs.reasoning_effort to be normalized, got %q", got)
	}
	if got := cfg.Loop.ReasoningEffort; got != "auto" {
		t.Fatalf("expected loop.reasoning_effort to be normalized, got %q", got)
	}
	if len(cfg.Specs.RunnerArgs) != 2 {
		t.Fatalf("expected specs.runner_args to trim blanks, got %#v", cfg.Specs.RunnerArgs)
	}
	if len(cfg.Loop.RunnerArgs) != 2 {
		t.Fatalf("expected loop.runner_args to trim blanks, got %#v", cfg.Loop.RunnerArgs)
	}
}

func TestRedactionModeValidation(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	base.Logging.RedactionMode = redaction.Mode("bad_mode")
	if err := base.Validate(); err == nil {
		t.Fatalf("expected validation error for logging.redaction_mode")
	}
}

func TestApplyPartialNormalizesRedactionMode(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	mode := redaction.Mode(" ALL_ENV ")
	partial := PartialConfig{
		Logging: &LoggingPartial{
			RedactionMode: &mode,
		},
	}
	cfg, err := ApplyPartial(base, partial, ".")
	if err != nil {
		t.Fatalf("ApplyPartial failed: %v", err)
	}
	if cfg.Logging.RedactionMode != redaction.ModeAllEnv {
		t.Fatalf("expected logging.redaction_mode normalized to %q, got %q", redaction.ModeAllEnv, cfg.Logging.RedactionMode)
	}
}

func TestLoadIgnoresDeprecatedFields(t *testing.T) {
	tmpDir := t.TempDir()
	repoDir := filepath.Join(tmpDir, "repo")
	cwd := filepath.Join(repoDir, "work")

	mustMkdirAll(t, filepath.Join(repoDir, ".ralph"))
	mustMkdirAll(t, cwd)

	repoConfigPath := filepath.Join(repoDir, ".ralph", "ralph.json")
	writeJSON(t, repoConfigPath, map[string]any{
		"runner": map[string]any{
			"max_workers": 4,
			"dry_run":     true,
		},
		"loop": map[string]any{
			"workers":       6,
			"poll_seconds":  10,
			"sleep_seconds": 2,
		},
		"git": map[string]any{
			"require_clean": true,
			"commit_prefix": "RQ",
			"auto_commit":   false,
			"auto_push":     true,
		},
		"ui": map[string]any{
			"theme": "solar",
		},
	})

	cfg, err := LoadFromLocations(LoadOptions{
		Locations: paths.Locations{
			CWD:            cwd,
			RepoRoot:       repoDir,
			RepoConfigPath: repoConfigPath,
		},
	})
	if err != nil {
		t.Fatalf("LoadFromLocations failed: %v", err)
	}
	if cfg.UI.Theme != "solar" {
		t.Fatalf("expected ui.theme to apply, got %q", cfg.UI.Theme)
	}
	if cfg.Git.AutoCommit {
		t.Fatalf("expected git.auto_commit to be false")
	}
	if !cfg.Git.AutoPush {
		t.Fatalf("expected git.auto_push to be true")
	}
	if cfg.Loop.SleepSeconds != 2 {
		t.Fatalf("expected loop.sleep_seconds to be 2, got %d", cfg.Loop.SleepSeconds)
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
