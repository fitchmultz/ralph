// Package migrate ensures the Ralph pin layout is initialized and configured.
// Entrypoint: Run.
package migrate

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

// Result captures migration outcomes.
type Result struct {
	RepoRoot     string
	PinDir       string
	CacheDir     string
	ConfigPath   string
	ConfigPinDir string
	Created      []string
	Overwritten  []string
}

// Run ensures the pin layout exists, updates repo config, and validates the pin files.
func Run(repoRoot string, repoConfigPath string) (Result, error) {
	cleanRoot := filepath.Clean(strings.TrimSpace(repoRoot))
	if cleanRoot == "" {
		return Result{}, fmt.Errorf("repo root required")
	}

	result := Result{
		RepoRoot: cleanRoot,
		PinDir:   filepath.Join(cleanRoot, ".ralph", "pin"),
	}
	if repoConfigPath == "" {
		repoConfigPath = filepath.Join(cleanRoot, ".ralph", "ralph.json")
	}
	result.ConfigPath = repoConfigPath

	relPin := relativePath(cleanRoot, result.PinDir)
	result.ConfigPinDir = relPin

	partial, err := config.LoadPartial(repoConfigPath)
	if err != nil {
		return result, err
	}
	updated := config.PartialConfig{}
	if partial != nil {
		updated = *partial
	}
	resolvedProjectType := project.DefaultType()
	if updated.ProjectType != nil {
		resolvedProjectType, err = project.ResolveType(*updated.ProjectType)
		if err != nil {
			return result, fmt.Errorf("project_type must be code or docs")
		}
	} else {
		detected, _, err := project.DetectType(cleanRoot)
		if err != nil {
			return result, err
		}
		resolvedProjectType = detected
	}
	if updated.Paths == nil {
		updated.Paths = &config.PathsPartial{}
	}
	pinPath := result.PinDir
	updated.Paths.PinDir = &pinPath
	updated.ProjectType = &resolvedProjectType

	if err := config.SavePartial(repoConfigPath, updated, config.SaveOptions{RelativeRoot: cleanRoot}); err != nil {
		return result, err
	}

	base, err := config.DefaultConfig()
	if err != nil {
		return result, err
	}
	cfg := base
	if partial != nil {
		cfg, err = config.ApplyPartial(base, *partial, cleanRoot, cleanRoot)
		if err != nil {
			return result, err
		}
	} else {
		cfg, err = config.ResolvePaths(base, cleanRoot, cleanRoot)
		if err != nil {
			return result, err
		}
	}

	initResult, err := pin.InitLayout(result.PinDir, cfg.Paths.CacheDir, pin.InitOptions{
		Force:       false,
		ProjectType: resolvedProjectType,
	})
	if err != nil {
		return result, err
	}
	result.CacheDir = initResult.CacheDir
	result.Created = initResult.Created
	result.Overwritten = initResult.Overwritten

	if _, err := pin.EnsureSpecsTemplate(result.PinDir, resolvedProjectType); err != nil {
		return result, err
	}
	files := pin.ResolveFiles(result.PinDir)
	if err := pin.ValidatePin(files, resolvedProjectType); err != nil {
		return result, err
	}

	return result, nil
}

func relativePath(base string, target string) string {
	rel, err := filepath.Rel(base, target)
	if err != nil || strings.HasPrefix(rel, "..") {
		return target
	}
	return rel
}
