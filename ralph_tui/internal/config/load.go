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

	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/paths"
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
	base = resolveConfigPaths(base, repoRoot)

	merged := base

	if opts.Locations.GlobalConfigPath != "" {
		partial, err := loadPartialFromFile(opts.Locations.GlobalConfigPath)
		if err != nil {
			return Config{}, err
		}
		if partial != nil {
			merged, err = applyPartial(merged, *partial, opts.Locations.HomeDir)
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
			merged, err = applyPartial(merged, *partial, repoRoot)
			if err != nil {
				return Config{}, err
			}
		}
	}

	if !isEmptyPartial(opts.CLIOverrides) {
		merged, err = applyPartial(merged, opts.CLIOverrides, opts.Locations.CWD)
		if err != nil {
			return Config{}, err
		}
	}

	if !isEmptyPartial(opts.SessionOverrides) {
		merged, err = applyPartial(merged, opts.SessionOverrides, opts.Locations.CWD)
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

// ApplyPartial merges a partial config onto a base config using the supplied basePath.
func ApplyPartial(base Config, partial PartialConfig, basePath string) (Config, error) {
	return applyPartial(base, partial, basePath)
}

// ResolvePaths resolves relative paths in a base config using the supplied basePath.
func ResolvePaths(cfg Config, basePath string) Config {
	return resolveConfigPaths(cfg, basePath)
}

func loadPartialFromFile(path string) (*PartialConfig, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}

	var partial PartialConfig
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&partial); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	return &partial, nil
}

func isEmptyPartial(partial PartialConfig) bool {
	if partial.Version != nil || partial.UI != nil || partial.Logging != nil || partial.Paths != nil {
		return false
	}
	if partial.Runner != nil || partial.Specs != nil || partial.Loop != nil || partial.Git != nil {
		return false
	}
	return true
}

func applyPartial(base Config, partial PartialConfig, basePath string) (Config, error) {
	if partial.Version != nil {
		base.Version = *partial.Version
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
	}
	if partial.Paths != nil {
		if partial.Paths.DataDir != nil {
			resolved, err := resolvePath(basePath, *partial.Paths.DataDir)
			if err != nil {
				return base, err
			}
			base.Paths.DataDir = resolved
		}
		if partial.Paths.CacheDir != nil {
			resolved, err := resolvePath(basePath, *partial.Paths.CacheDir)
			if err != nil {
				return base, err
			}
			base.Paths.CacheDir = resolved
		}
		if partial.Paths.PinDir != nil {
			resolved, err := resolvePath(basePath, *partial.Paths.PinDir)
			if err != nil {
				return base, err
			}
			base.Paths.PinDir = resolved
		}
	}
	if partial.Runner != nil {
		if partial.Runner.MaxWorkers != nil {
			base.Runner.MaxWorkers = *partial.Runner.MaxWorkers
		}
		if partial.Runner.DryRun != nil {
			base.Runner.DryRun = *partial.Runner.DryRun
		}
	}
	if partial.Specs != nil {
		if partial.Specs.AutofillScout != nil {
			base.Specs.AutofillScout = *partial.Specs.AutofillScout
		}
	}
	if partial.Loop != nil {
		if partial.Loop.Workers != nil {
			base.Loop.Workers = *partial.Loop.Workers
		}
		if partial.Loop.PollSeconds != nil {
			base.Loop.PollSeconds = *partial.Loop.PollSeconds
		}
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
		if partial.Loop.OnlyTags != nil {
			base.Loop.OnlyTags = strings.TrimSpace(*partial.Loop.OnlyTags)
		}
		if partial.Loop.RequireMain != nil {
			base.Loop.RequireMain = *partial.Loop.RequireMain
		}
	}
	if partial.Git != nil {
		if partial.Git.AutoCommit != nil {
			base.Git.AutoCommit = *partial.Git.AutoCommit
		}
		if partial.Git.AutoPush != nil {
			base.Git.AutoPush = *partial.Git.AutoPush
		}
		if partial.Git.RequireClean != nil {
			base.Git.RequireClean = *partial.Git.RequireClean
		}
		if partial.Git.CommitPrefix != nil {
			base.Git.CommitPrefix = strings.TrimSpace(*partial.Git.CommitPrefix)
		}
	}

	return base, nil
}

func resolveConfigPaths(cfg Config, basePath string) Config {
	if resolved, err := resolvePath(basePath, cfg.Paths.DataDir); err == nil {
		cfg.Paths.DataDir = resolved
	}
	if resolved, err := resolvePath(basePath, cfg.Paths.CacheDir); err == nil {
		cfg.Paths.CacheDir = resolved
	}
	if resolved, err := resolvePath(basePath, cfg.Paths.PinDir); err == nil {
		cfg.Paths.PinDir = resolved
	}

	return cfg
}

func resolvePath(basePath string, value string) (string, error) {
	clean := strings.TrimSpace(value)
	if clean == "" {
		return "", fmt.Errorf("path cannot be empty")
	}
	if strings.HasPrefix(clean, "~") {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
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
