// Package migrate relocates legacy Ralph pin files into the .ralph/pin layout.
// Entrypoint: Run.
package migrate

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/config"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/pin"
)

// Result captures migration outcomes.
type Result struct {
	RepoRoot     string
	OldPinDir    string
	NewPinDir    string
	ConfigPath   string
	ConfigPinDir string
	Moved        []string
}

// Run performs the pin migration and validates the resulting pin files.
func Run(repoRoot string, repoConfigPath string) (Result, error) {
	cleanRoot := filepath.Clean(strings.TrimSpace(repoRoot))
	if cleanRoot == "" {
		return Result{}, fmt.Errorf("repo root required")
	}

	result := Result{
		RepoRoot:  cleanRoot,
		OldPinDir: filepath.Join(cleanRoot, "ralph_legacy", "specs"),
		NewPinDir: filepath.Join(cleanRoot, ".ralph", "pin"),
	}
	if repoConfigPath == "" {
		repoConfigPath = filepath.Join(cleanRoot, ".ralph", "ralph.json")
	}
	result.ConfigPath = repoConfigPath

	moved, err := movePinDir(result.OldPinDir, result.NewPinDir)
	if err != nil {
		return result, err
	}
	result.Moved = moved

	if !dirExists(result.NewPinDir) {
		return result, fmt.Errorf("pin directory not found at %s", result.NewPinDir)
	}

	relPin := relativePath(cleanRoot, result.NewPinDir)
	result.ConfigPinDir = relPin

	partial, err := config.LoadPartial(repoConfigPath)
	if err != nil {
		return result, err
	}
	updated := config.PartialConfig{}
	if partial != nil {
		updated = *partial
	}
	if updated.Paths == nil {
		updated.Paths = &config.PathsPartial{}
	}
	pinPath := result.NewPinDir
	updated.Paths.PinDir = &pinPath

	if err := config.SavePartial(repoConfigPath, updated, config.SaveOptions{RelativeRoot: cleanRoot}); err != nil {
		return result, err
	}

	files := pin.ResolveFiles(result.NewPinDir, cleanRoot)
	if err := pin.ValidatePin(files); err != nil {
		return result, err
	}

	return result, nil
}

func movePinDir(oldDir string, newDir string) ([]string, error) {
	if !dirExists(oldDir) {
		return nil, nil
	}
	if err := os.MkdirAll(newDir, 0o700); err != nil {
		return nil, err
	}
	entries, err := os.ReadDir(oldDir)
	if err != nil {
		return nil, err
	}
	moved := make([]string, 0, len(entries))
	for _, entry := range entries {
		src := filepath.Join(oldDir, entry.Name())
		dst := filepath.Join(newDir, entry.Name())
		if _, err := os.Stat(dst); err == nil {
			return nil, fmt.Errorf("target already exists: %s", dst)
		} else if !os.IsNotExist(err) {
			return nil, err
		}
		if err := os.Rename(src, dst); err != nil {
			return nil, err
		}
		moved = append(moved, entry.Name())
	}
	if empty, err := isEmptyDir(oldDir); err == nil && empty {
		_ = os.Remove(oldDir)
	}
	sort.Strings(moved)
	return moved, nil
}

func dirExists(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return info.IsDir()
}

func isEmptyDir(path string) (bool, error) {
	entries, err := os.ReadDir(path)
	if err != nil {
		return false, err
	}
	return len(entries) == 0, nil
}

func relativePath(base string, target string) string {
	rel, err := filepath.Rel(base, target)
	if err != nil || strings.HasPrefix(rel, "..") {
		return target
	}
	return rel
}
