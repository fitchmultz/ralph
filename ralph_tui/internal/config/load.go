// Package config loads and merges configuration layers with deterministic precedence.
// Entrypoint: Load and LoadFromLocations.
package config

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

// LoadOptions controls how configuration is resolved.
type LoadOptions struct {
	Locations        paths.Locations
	SessionOverrides PartialConfig
	CLIOverrides     PartialConfig
}

// Load resolves configuration using repo detection from the provided cwd (or os.Getwd if empty).
func Load(cwd string, sessionOverrides PartialConfig, cliOverrides PartialConfig) (Config, error) {
	locs, err := paths.Resolve(cwd)
	if err != nil {
		return Config{}, err
	}

	return LoadFromLocations(LoadOptions{
		Locations:        locs,
		SessionOverrides: sessionOverrides,
		CLIOverrides:     cliOverrides,
	})
}

// LoadFromLocations resolves configuration using explicit locations.
func LoadFromLocations(opts LoadOptions) (Config, error) {
	base, err := DefaultConfig()
	if err != nil {
		return Config{}, err
	}

	repoRoot := opts.Locations.RepoRoot
	if repoRoot == "" {
		repoRoot = opts.Locations.CWD
	}
	base, err = resolveConfigPaths(base, repoRoot, repoRoot)
	if err != nil {
		return Config{}, err
	}

	merged := base

	if opts.Locations.GlobalConfigPath != "" {
		partial, err := loadPartialFromFile(opts.Locations.GlobalConfigPath)
		if err != nil {
			return Config{}, err
		}
		if partial != nil {
			merged, err = applyPartial(merged, *partial, opts.Locations.HomeDir, repoRoot)
			if err != nil {
				return Config{}, err
			}
		}
	}

	if opts.Locations.RepoConfigPath != "" {
		partial, err := loadPartialFromFile(opts.Locations.RepoConfigPath)
		if err != nil {
			return Config{}, err
		}
		if partial != nil {
			merged, err = applyPartial(merged, *partial, repoRoot, repoRoot)
			if err != nil {
				return Config{}, err
			}
		}
	}

	if !isEmptyPartial(opts.CLIOverrides) {
		merged, err = applyPartial(merged, opts.CLIOverrides, opts.Locations.CWD, repoRoot)
		if err != nil {
			return Config{}, err
		}
	}

	if !isEmptyPartial(opts.SessionOverrides) {
		merged, err = applyPartial(merged, opts.SessionOverrides, opts.Locations.CWD, repoRoot)
		if err != nil {
			return Config{}, err
		}
	}

	if err := merged.Validate(); err != nil {
		return Config{}, err
	}

	return merged, nil
}

// LoadPartial reads a partial config from disk if it exists.
func LoadPartial(path string) (*PartialConfig, error) {
	return loadPartialFromFile(path)
}

// ApplyPartial merges a partial config onto a base config using the supplied basePath and repoRoot.
func ApplyPartial(base Config, partial PartialConfig, basePath string, repoRoot string) (Config, error) {
	return applyPartial(base, partial, basePath, repoRoot)
}

// ResolvePaths resolves relative paths in a base config using the supplied basePath and repoRoot.
// Returns an error when resolution fails.
func ResolvePaths(cfg Config, basePath string, repoRoot string) (Config, error) {
	return resolveConfigPaths(cfg, basePath, repoRoot)
}

func loadPartialFromFile(path string) (*PartialConfig, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}

	cleaned, err := stripDeprecatedConfigFields(data)
	if err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	var partial PartialConfig
	decoder := json.NewDecoder(strings.NewReader(string(cleaned)))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&partial); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	return &partial, nil
}

func isEmptyPartial(partial PartialConfig) bool {
	if partial.Version != nil || partial.ProjectType != nil || partial.UI != nil || partial.Logging != nil || partial.Paths != nil {
		return false
	}
	if partial.Specs != nil || partial.Loop != nil || partial.Git != nil {
		return false
	}
	return true
}

func applyPartial(base Config, partial PartialConfig, basePath string, repoRoot string) (Config, error) {
	if partial.Version != nil {
		base.Version = *partial.Version
	}
	if partial.ProjectType != nil {
		base.ProjectType = project.NormalizeType(string(*partial.ProjectType))
	}
	if partial.UI != nil {
		if partial.UI.Theme != nil {
			base.UI.Theme = strings.TrimSpace(*partial.UI.Theme)
		}
		if partial.UI.RefreshSeconds != nil {
			base.UI.RefreshSeconds = *partial.UI.RefreshSeconds
		}
	}
	if partial.Logging != nil {
		if partial.Logging.Level != nil {
			base.Logging.Level = strings.ToLower(strings.TrimSpace(*partial.Logging.Level))
		}
		if partial.Logging.File != nil {
			trimmed := strings.TrimSpace(*partial.Logging.File)
			if trimmed == "" {
				base.Logging.File = ""
			} else {
				resolved, err := resolvePathWithRepo("logging.file", basePath, repoRoot, trimmed)
				if err != nil {
					return base, err
				}
				base.Logging.File = resolved
			}
		}
		if partial.Logging.RedactionMode != nil {
			base.Logging.RedactionMode = redaction.NormalizeMode(string(*partial.Logging.RedactionMode))
		}
		if partial.Logging.MaxBufferedBytes != nil {
			base.Logging.MaxBufferedBytes = *partial.Logging.MaxBufferedBytes
		}
	}
	if partial.Paths != nil {
		if partial.Paths.DataDir != nil {
			resolved, err := resolvePathWithRepo("paths.data_dir", basePath, repoRoot, *partial.Paths.DataDir)
			if err != nil {
				return base, err
			}
			base.Paths.DataDir = resolved
		}
		if partial.Paths.CacheDir != nil {
			resolved, err := resolvePathWithRepo("paths.cache_dir", basePath, repoRoot, *partial.Paths.CacheDir)
			if err != nil {
				return base, err
			}
			base.Paths.CacheDir = resolved
		}
		if partial.Paths.PinDir != nil {
			resolved, err := resolvePathWithRepo("paths.pin_dir", basePath, repoRoot, *partial.Paths.PinDir)
			if err != nil {
				return base, err
			}
			base.Paths.PinDir = resolved
		}
	}
	if partial.Specs != nil {
		if partial.Specs.AutofillScout != nil {
			base.Specs.AutofillScout = *partial.Specs.AutofillScout
		}
		if partial.Specs.ScoutWorkflow != nil {
			base.Specs.ScoutWorkflow = *partial.Specs.ScoutWorkflow
		}
		if partial.Specs.UserFocus != nil {
			base.Specs.UserFocus = strings.TrimSpace(*partial.Specs.UserFocus)
		}
		if partial.Specs.Runner != nil {
			base.Specs.Runner = runnerargs.NormalizeRunner(*partial.Specs.Runner)
		}
		if partial.Specs.RunnerArgs != nil {
			base.Specs.RunnerArgs = runnerargs.NormalizeArgs(partial.Specs.RunnerArgs)
		}
		if partial.Specs.ReasoningEffort != nil {
			base.Specs.ReasoningEffort = strings.ToLower(strings.TrimSpace(*partial.Specs.ReasoningEffort))
		}
	}
	if partial.Loop != nil {
		if partial.Loop.SleepSeconds != nil {
			base.Loop.SleepSeconds = *partial.Loop.SleepSeconds
		}
		if partial.Loop.MaxIterations != nil {
			base.Loop.MaxIterations = *partial.Loop.MaxIterations
		}
		if partial.Loop.MaxStalled != nil {
			base.Loop.MaxStalled = *partial.Loop.MaxStalled
		}
		if partial.Loop.MaxRepairAttempts != nil {
			base.Loop.MaxRepairAttempts = *partial.Loop.MaxRepairAttempts
		}
		if partial.Loop.RunnerInactivitySeconds != nil {
			base.Loop.RunnerInactivitySeconds = *partial.Loop.RunnerInactivitySeconds
		}
		if partial.Loop.OnlyTags != nil {
			base.Loop.OnlyTags = strings.TrimSpace(*partial.Loop.OnlyTags)
		}
		if partial.Loop.RequireMain != nil {
			base.Loop.RequireMain = *partial.Loop.RequireMain
		}
		if partial.Loop.Runner != nil {
			base.Loop.Runner = runnerargs.NormalizeRunner(*partial.Loop.Runner)
		}
		if partial.Loop.RunnerArgs != nil {
			base.Loop.RunnerArgs = runnerargs.NormalizeArgs(partial.Loop.RunnerArgs)
		}
		if partial.Loop.ReasoningEffort != nil {
			base.Loop.ReasoningEffort = strings.ToLower(strings.TrimSpace(*partial.Loop.ReasoningEffort))
		}
		if partial.Loop.DirtyRepo != nil {
			if partial.Loop.DirtyRepo.StartPolicy != nil {
				base.Loop.DirtyRepo.StartPolicy = strings.ToLower(strings.TrimSpace(*partial.Loop.DirtyRepo.StartPolicy))
			}
			if partial.Loop.DirtyRepo.DuringPolicy != nil {
				base.Loop.DirtyRepo.DuringPolicy = strings.ToLower(strings.TrimSpace(*partial.Loop.DirtyRepo.DuringPolicy))
			}
			if partial.Loop.DirtyRepo.AllowUntracked != nil {
				base.Loop.DirtyRepo.AllowUntracked = *partial.Loop.DirtyRepo.AllowUntracked
			}
			if partial.Loop.DirtyRepo.QuarantineCleanUntracked != nil {
				base.Loop.DirtyRepo.QuarantineCleanUntracked = *partial.Loop.DirtyRepo.QuarantineCleanUntracked
			}
		}
	}
	if partial.Git != nil {
		if partial.Git.AutoCommit != nil {
			base.Git.AutoCommit = *partial.Git.AutoCommit
		}
		if partial.Git.AutoPush != nil {
			base.Git.AutoPush = *partial.Git.AutoPush
		}
	}

	return base, nil
}

func stripDeprecatedConfigFields(data []byte) ([]byte, error) {
	var payload map[string]any
	if err := json.Unmarshal(data, &payload); err != nil {
		return nil, err
	}

	delete(payload, "runner")

	if loopValue, ok := payload["loop"].(map[string]any); ok {
		delete(loopValue, "workers")
		delete(loopValue, "poll_seconds")
		if len(loopValue) == 0 {
			delete(payload, "loop")
		}
	}

	if gitValue, ok := payload["git"].(map[string]any); ok {
		delete(gitValue, "require_clean")
		delete(gitValue, "commit_prefix")
		if len(gitValue) == 0 {
			delete(payload, "git")
		}
	}

	cleaned, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}
	return cleaned, nil
}

func resolveConfigPaths(cfg Config, basePath string, repoRoot string) (Config, error) {
	resolved, err := resolvePathWithRepo("paths.data_dir", basePath, repoRoot, cfg.Paths.DataDir)
	if err != nil {
		return cfg, err
	}
	cfg.Paths.DataDir = resolved

	resolved, err = resolvePathWithRepo("paths.cache_dir", basePath, repoRoot, cfg.Paths.CacheDir)
	if err != nil {
		return cfg, err
	}
	cfg.Paths.CacheDir = resolved

	resolved, err = resolvePathWithRepo("paths.pin_dir", basePath, repoRoot, cfg.Paths.PinDir)
	if err != nil {
		return cfg, err
	}
	cfg.Paths.PinDir = resolved

	if strings.TrimSpace(cfg.Logging.File) != "" {
		resolved, err = resolvePathWithRepo("logging.file", basePath, repoRoot, cfg.Logging.File)
		if err != nil {
			return cfg, err
		}
		cfg.Logging.File = resolved
	}

	return cfg, nil
}

func resolvePathWithRepo(field string, basePath string, repoRoot string, value string) (string, error) {
	clean := strings.TrimSpace(value)
	if clean == "" {
		return "", fmt.Errorf("resolve %s: path cannot be empty", field)
	}
	if strings.Contains(clean, "{repo}") {
		repoName := candidateRepoName(repoRoot)
		if repoName == "" {
			repoName = candidateRepoName(basePath)
		}
		if repoName == "" {
			return "", fmt.Errorf("resolve %s: unknown repo root", field)
		}
		clean = strings.ReplaceAll(clean, "{repo}", repoName)
	}
	if strings.HasPrefix(clean, "~") {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", fmt.Errorf("resolve %s: %w", field, err)
		}
		if clean == "~" {
			clean = home
		} else if strings.HasPrefix(clean, "~/") {
			clean = filepath.Join(home, strings.TrimPrefix(clean, "~/"))
		}
	}
	if filepath.IsAbs(clean) {
		return filepath.Clean(clean), nil
	}
	if basePath == "" {
		return filepath.Clean(clean), nil
	}
	return filepath.Clean(filepath.Join(basePath, clean)), nil
}

func candidateRepoName(path string) string {
	trimmed := strings.TrimSpace(path)
	if trimmed == "" {
		return ""
	}
	cleaned := filepath.Clean(trimmed)
	if cleaned == "." || cleaned == string(filepath.Separator) {
		return ""
	}
	base := filepath.Base(cleaned)
	if base == "" || base == "." || base == string(filepath.Separator) {
		return ""
	}
	if strings.HasSuffix(base, ":") && len(base) == 2 {
		return ""
	}
	return base
}
