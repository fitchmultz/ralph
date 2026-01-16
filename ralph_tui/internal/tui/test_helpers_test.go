// Package tui provides test helpers for hermetic TUI model construction.
package tui

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
)

func newHermeticModel(t *testing.T) (model, paths.Locations, config.Config) {
	t.Helper()

	repoRoot := t.TempDir()
	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	if err := os.MkdirAll(pinDir, 0o755); err != nil {
		t.Fatalf("create pin dir: %v", err)
	}

	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0001 [ui]: Sample task (tui)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(pinDir, "implementation_queue.md"), queueContent)
	writeTestFile(t, filepath.Join(pinDir, "implementation_done.md"), "## Done\n")
	writeTestFile(t, filepath.Join(pinDir, "lookup_table.md"), "## Lookup\n")
	writeTestFile(t, filepath.Join(pinDir, "README.md"), "Ralph pin fixtures.\n")
	writeTestFile(t, filepath.Join(pinDir, "specs_builder.md"), "Use AGENTS.md for instructions.\n\n{{INTERACTIVE_INSTRUCTIONS}}\n\n{{INNOVATE_INSTRUCTIONS}}\n")

	ensureGit(t)
	runGit(t, repoRoot, "init", "-b", "main")
	runGit(t, repoRoot, "config", "user.email", "test@example.com")
	runGit(t, repoRoot, "config", "user.name", "Test User")
	runGit(t, repoRoot, "add", ".")
	runGit(t, repoRoot, "commit", "-m", "init")

	base, err := config.DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}
	cfg, err := config.ResolvePaths(base, repoRoot, repoRoot)
	if err != nil {
		t.Fatalf("ResolvePaths failed: %v", err)
	}
	if err := cfg.Validate(); err != nil {
		t.Fatalf("validate config: %v", err)
	}

	locs := paths.Locations{
		CWD:              repoRoot,
		RepoRoot:         repoRoot,
		HomeDir:          repoRoot,
		GlobalConfigPath: "",
		RepoConfigPath:   "",
	}

	return newModel(cfg, locs, StartOptions{}), locs, cfg
}

func writeTestFile(t *testing.T, path string, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

func mustReadFile(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return data
}

func ensureGit(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skipf("missing git: %v", err)
	}
}

func runGit(t *testing.T, repoRoot string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = repoRoot
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v failed: %v\n%s", args, err, string(output))
	}
	return string(output)
}

var errSentinel = errors.New("sentinel error")

func newTestKeyMap() keyMap {
	return newKeyMap()
}
