// Package config provides tests for configuration loading and validation.
// Entrypoint: go test ./...
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
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
	base, err = ResolvePaths(base, tmpDir, tmpDir)
	if err != nil {
		t.Fatalf("ResolvePaths failed: %v", err)
	}

	partial := PartialConfig{
		Logging: &LoggingPartial{File: stringPtr("logs/ralph_tui.log")},
	}
	cfg, err := ApplyPartial(base, partial, tmpDir, tmpDir)
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

func TestResolvePathsExpandsRepoToken(t *testing.T) {
	tmpDir := t.TempDir()
	homeDir := filepath.Join(tmpDir, "home")
	if err := os.MkdirAll(homeDir, 0o755); err != nil {
		t.Fatalf("create home dir: %v", err)
	}
	t.Setenv("HOME", homeDir)
	if runtime.GOOS == "windows" {
		t.Setenv("USERPROFILE", homeDir)
	}

	repoRoot := filepath.Join(tmpDir, "repo")
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}
	cfg, err := ResolvePaths(base, repoRoot, repoRoot)
	if err != nil {
		t.Fatalf("ResolvePaths failed: %v", err)
	}

	expected := filepath.Join(homeDir, ".ralph", "cache", filepath.Base(repoRoot))
	if cfg.Paths.CacheDir != expected {
		t.Fatalf("expected cache_dir %q, got %q", expected, cfg.Paths.CacheDir)
	}
}

func TestResolvePathWithRepo_RepoTokenExpansion(t *testing.T) {
	tmpDir := t.TempDir()
	repoRoot := filepath.Join(tmpDir, "myrepo")
	basePath := filepath.Join(tmpDir, "workdir")
	relativeRoot := filepath.Join(tmpDir, "relative-root")

	cases := []struct {
		name       string
		repoRoot   string
		basePath   string
		value      string
		want       string
		wantErrStr string
	}{
		{
			name:     "uses repo root basename",
			repoRoot: repoRoot,
			basePath: repoRoot,
			value:    "cache/{repo}",
			want:     filepath.Join(repoRoot, "cache", filepath.Base(repoRoot)),
		},
		{
			name:     "falls back to base path",
			repoRoot: string(filepath.Separator),
			basePath: basePath,
			value:    "cache/{repo}",
			want:     filepath.Join(basePath, "cache", filepath.Base(basePath)),
		},
		{
			name:       "errors when repo root is unknown",
			repoRoot:   string(filepath.Separator),
			basePath:   ".",
			value:      "cache/{repo}",
			wantErrStr: "unknown repo root",
		},
		{
			name:     "handles nested relative base path",
			repoRoot: relativeRoot,
			basePath: relativeRoot,
			value:    "pin/{repo}",
			want:     filepath.Join(relativeRoot, "pin", filepath.Base(relativeRoot)),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			resolved, err := resolvePathWithRepo("paths.cache_dir", tc.basePath, tc.repoRoot, tc.value)
			if tc.wantErrStr != "" {
				if err == nil {
					t.Fatalf("expected error, got nil")
				}
				if !strings.Contains(err.Error(), tc.wantErrStr) {
					t.Fatalf("expected error to contain %q, got %v", tc.wantErrStr, err)
				}
				if !strings.Contains(err.Error(), "resolve paths.cache_dir") {
					t.Fatalf("expected error to include field context, got %v", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if resolved != tc.want {
				t.Fatalf("expected %q, got %q", tc.want, resolved)
			}
		})
	}
}

func TestSavePartial_RelativeRoot_RoundTripLoad(t *testing.T) {
	tmpDir := t.TempDir()
	repoRoot := filepath.Join(tmpDir, "repo")
	homeDir := filepath.Join(tmpDir, "home")
	cwd := filepath.Join(repoRoot, "work")

	mustMkdirAll(t, filepath.Join(repoRoot, ".ralph"))
	mustMkdirAll(t, cwd)
	mustMkdirAll(t, homeDir)

	t.Setenv("HOME", homeDir)
	if runtime.GOOS == "windows" {
		t.Setenv("USERPROFILE", homeDir)
	}

	inside := filepath.Join(repoRoot, "data")
	outside := filepath.Join(tmpDir, "outside")
	rootPath := repoRoot

	partial := PartialConfig{
		Paths: &PathsPartial{
			DataDir:  &inside,
			CacheDir: &outside,
			PinDir:   &rootPath,
		},
	}

	repoConfigPath := filepath.Join(repoRoot, ".ralph", "ralph.json")
	if err := SavePartial(repoConfigPath, partial, SaveOptions{RelativeRoot: repoRoot}); err != nil {
		t.Fatalf("SavePartial failed: %v", err)
	}

	data, err := os.ReadFile(repoConfigPath)
	if err != nil {
		t.Fatalf("read config: %v", err)
	}
	var saved PartialConfig
	if err := json.Unmarshal(data, &saved); err != nil {
		t.Fatalf("unmarshal config: %v", err)
	}
	if saved.Paths == nil {
		t.Fatalf("expected saved paths config")
	}
	if saved.Paths.DataDir == nil || *saved.Paths.DataDir != filepath.Join("data") {
		t.Fatalf("expected data_dir to be relative, got %#v", saved.Paths.DataDir)
	}
	if saved.Paths.CacheDir == nil || *saved.Paths.CacheDir != outside {
		t.Fatalf("expected cache_dir to remain absolute, got %#v", saved.Paths.CacheDir)
	}
	if saved.Paths.PinDir == nil || *saved.Paths.PinDir != repoRoot {
		t.Fatalf("expected pin_dir to remain absolute, got %#v", saved.Paths.PinDir)
	}

	cfg, err := LoadFromLocations(LoadOptions{
		Locations: paths.Locations{
			CWD:            cwd,
			RepoRoot:       repoRoot,
			RepoConfigPath: repoConfigPath,
			HomeDir:        homeDir,
		},
	})
	if err != nil {
		t.Fatalf("LoadFromLocations failed: %v", err)
	}

	if cfg.Paths.DataDir != filepath.Join(repoRoot, "data") {
		t.Fatalf("expected data_dir %q, got %q", filepath.Join(repoRoot, "data"), cfg.Paths.DataDir)
	}
	if cfg.Paths.CacheDir != outside {
		t.Fatalf("expected cache_dir %q, got %q", outside, cfg.Paths.CacheDir)
	}
	if cfg.Paths.PinDir != repoRoot {
		t.Fatalf("expected pin_dir %q, got %q", repoRoot, cfg.Paths.PinDir)
	}
}

func TestApplyPartialNormalizesRunnerSettings(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	partial := PartialConfig{
		ProjectType: projectTypePtr(" Docs "),
		Specs: &SpecsPartial{
			Runner:          stringPtr(" Codex "),
			RunnerArgs:      []string{"  -c", "model_reasoning_effort=\"high\" ", " ", ""},
			ReasoningEffort: stringPtr(" High "),
		},
		Loop: &LoopPartial{
			Runner:          stringPtr(" OPENcode "),
			RunnerArgs:      []string{" --flag", "value ", "", "  "},
			ReasoningEffort: stringPtr(" AUTO "),
		},
	}

	cfg, err := ApplyPartial(base, partial, ".", ".")
	if err != nil {
		t.Fatalf("ApplyPartial failed: %v", err)
	}
	if got := cfg.Specs.ReasoningEffort; got != "high" {
		t.Fatalf("expected specs.reasoning_effort to be normalized, got %q", got)
	}
	if got := cfg.Specs.Runner; got != "codex" {
		t.Fatalf("expected specs.runner to be normalized, got %q", got)
	}
	if got := cfg.ProjectType; got != project.TypeDocs {
		t.Fatalf("expected project_type normalized to %q, got %q", project.TypeDocs, got)
	}
	if got := cfg.Loop.ReasoningEffort; got != "auto" {
		t.Fatalf("expected loop.reasoning_effort to be normalized, got %q", got)
	}
	if got := cfg.Loop.Runner; got != "opencode" {
		t.Fatalf("expected loop.runner to be normalized, got %q", got)
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

func TestLoggingMaxBufferedBytesValidation(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	base.Logging.MaxBufferedBytes = -1
	if err := base.Validate(); err == nil {
		t.Fatalf("expected validation error for logging.max_buffered_bytes")
	}
}

func TestRunnerInactivityValidation(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	base.Loop.RunnerInactivitySeconds = -1
	if err := base.Validate(); err == nil {
		t.Fatalf("expected validation error for loop.runner_inactivity_seconds")
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
	cfg, err := ApplyPartial(base, partial, ".", ".")
	if err != nil {
		t.Fatalf("ApplyPartial failed: %v", err)
	}
	if cfg.Logging.RedactionMode != redaction.ModeAllEnv {
		t.Fatalf("expected logging.redaction_mode normalized to %q, got %q", redaction.ModeAllEnv, cfg.Logging.RedactionMode)
	}
}

func TestDirtyRepoPolicyValidation(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}
	base.Loop.DirtyRepo.StartPolicy = "bad"
	if err := base.Validate(); err == nil {
		t.Fatalf("expected validation error for loop.dirty_repo.start_policy")
	}
}

func TestProjectTypeValidation(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}
	base.ProjectType = project.Type("invalid")
	if err := base.Validate(); err == nil {
		t.Fatalf("expected validation error for project_type")
	}
}

func TestOnlyTagsValidation(t *testing.T) {
	base, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}
	tmpDir := t.TempDir()
	base, err = ResolvePaths(base, tmpDir, tmpDir)
	if err != nil {
		t.Fatalf("ResolvePaths failed: %v", err)
	}
	base.Loop.OnlyTags = "unknown"
	err = base.Validate()
	if err == nil {
		t.Fatalf("expected validation error for loop.only_tags")
	}
	if !strings.Contains(err.Error(), "loop.only_tags") {
		t.Fatalf("expected loop.only_tags error, got %v", err)
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

func projectTypePtr(value string) *project.Type {
	typed := project.Type(value)
	return &typed
}

func intPtr(value int) *int {
	return &value
}
