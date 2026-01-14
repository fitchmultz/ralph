// Package paths resolves repo and config locations for Ralph.
// Entrypoint: Resolve.
package paths

import (
	"bytes"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// Locations describes resolved filesystem locations relevant to Ralph.
type Locations struct {
	CWD              string
	RepoRoot         string
	RepoConfigPath   string
	HomeDir          string
	GlobalConfigPath string
}

// Resolve determines repo root and config paths using git and filesystem fallbacks.
func Resolve(cwd string) (Locations, error) {
	resolvedCWD := cwd
	if resolvedCWD == "" {
		current, err := os.Getwd()
		if err != nil {
			return Locations{}, err
		}
		resolvedCWD = current
	}

	repoRoot := gitRepoRoot(resolvedCWD)
	if repoRoot == "" {
		repoRoot = walkForGit(resolvedCWD)
	}
	if repoRoot == "" {
		repoRoot = resolvedCWD
	}

	homeDir, _ := os.UserHomeDir()

	locs := Locations{
		CWD:              resolvedCWD,
		RepoRoot:         repoRoot,
		RepoConfigPath:   filepath.Join(repoRoot, ".ralph", "ralph.json"),
		HomeDir:          homeDir,
		GlobalConfigPath: "",
	}
	if homeDir != "" {
		locs.GlobalConfigPath = filepath.Join(homeDir, ".ralph", "ralph.json")
	}

	return locs, nil
}

func gitRepoRoot(cwd string) string {
	cmd := exec.Command("git", "rev-parse", "--show-toplevel")
	cmd.Dir = cwd
	output, err := cmd.Output()
	if err != nil {
		return ""
	}
	trimmed := strings.TrimSpace(string(bytes.TrimSpace(output)))
	if trimmed == "" {
		return ""
	}
	return trimmed
}

func walkForGit(start string) string {
	path := start
	for {
		if hasGitDir(path) {
			return path
		}
		parent := filepath.Dir(path)
		if parent == path {
			return ""
		}
		path = parent
	}
}

func hasGitDir(dir string) bool {
	candidate := filepath.Join(dir, ".git")
	info, err := os.Stat(candidate)
	if err == nil {
		return info.IsDir() || isGitFile(info)
	}
	if errors.Is(err, os.ErrNotExist) {
		return false
	}
	return false
}

func isGitFile(info os.FileInfo) bool {
	return info.Mode().IsRegular() && info.Size() > 0
}
