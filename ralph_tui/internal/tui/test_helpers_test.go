// Package tui provides test helpers for hermetic TUI model construction.
package tui

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/muesli/termenv"
)

func newHermeticModel(t *testing.T) (model, paths.Locations, config.Config) {
	t.Helper()

	repoRoot := t.TempDir()
	t.Setenv("HOME", repoRoot)
	t.Setenv("USERPROFILE", repoRoot)
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
	writeTestFile(t, filepath.Join(pinDir, "specs_builder_docs.md"), "Use AGENTS.md for instructions.\n\n{{INTERACTIVE_INSTRUCTIONS}}\n\n{{INNOVATE_INSTRUCTIONS}}\n")

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

type terminalSize struct {
	w int
	h int
}

func renderContractSizes() []terminalSize {
	return []terminalSize{
		{w: 20, h: 8},
		{w: 30, h: 10},
		{w: 40, h: 10},
		{w: 48, h: 12},
		{w: 60, h: 20},
		{w: 80, h: 24},
		{w: 100, h: 40},
		{w: 120, h: 50},
	}
}

func dashboardNarrowSizes() []terminalSize {
	return []terminalSize{
		{w: 12, h: 5},
		{w: 15, h: 6},
		{w: 18, h: 6},
		{w: 20, h: 6},
	}
}

func dashboardRepoPanelSizes() []terminalSize {
	return []terminalSize{
		{w: 12, h: 5},
		{w: 18, h: 6},
		{w: 24, h: 8},
	}
}

func withAsciiColorProfile(t *testing.T, fn func()) {
	t.Helper()
	previous := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.Ascii)
	t.Cleanup(func() {
		lipgloss.SetColorProfile(previous)
	})
	fn()
}

func normalizeRender(rendered string) string {
	clean := ansi.Strip(rendered)
	lines := strings.Split(strings.TrimRight(clean, "\n"), "\n")
	for i, line := range lines {
		if idx := strings.Index(line, "Log path: "); idx >= 0 {
			tail := ""
			if borderIdx := strings.LastIndex(line, "│"); borderIdx > idx {
				tail = line[borderIdx:]
			}
			lines[i] = line[:idx] + "Log path: <log> " + tail
		}
	}
	return strings.Join(lines, "\n")
}

func assertSnapshot(t *testing.T, subDir string, name string, rendered string) {
	t.Helper()
	dir := filepath.Join("testdata", subDir)
	path := filepath.Join(dir, name+".txt")
	if os.Getenv("UPDATE_GOLDEN") != "" {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatalf("create snapshot dir: %v", err)
		}
		if err := os.WriteFile(path, []byte(normalizeRender(rendered)), 0o644); err != nil {
			t.Fatalf("write snapshot: %v", err)
		}
	}
	want := string(mustReadFile(t, path))
	got := normalizeRender(rendered)
	if got != want {
		t.Fatalf("snapshot mismatch for %s\n--- got ---\n%s\n--- want ---\n%s", path, got, want)
	}
}

func assertContainsLines(t *testing.T, rendered string, lines ...string) {
	t.Helper()
	clean := normalizeRender(rendered)
	for _, line := range lines {
		if !strings.Contains(clean, line) {
			t.Fatalf("expected to find %q in output:\n%s", line, clean)
		}
	}
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
