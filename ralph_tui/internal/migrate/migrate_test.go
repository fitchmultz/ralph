// Package migrate provides tests for the Ralph pin migration flow.
// Entrypoint: go test ./...
package migrate

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/pin"
)

func TestRunMigratesPinAndUpdatesConfig(t *testing.T) {
	repoRoot := t.TempDir()
	oldPin := filepath.Join(repoRoot, "ralph_legacy", "specs")
	writeMinimalPin(t, repoRoot, oldPin)

	configPath := filepath.Join(repoRoot, ".ralph", "ralph.json")
	result, err := Run(repoRoot, configPath)
	if err != nil {
		t.Fatalf("Run failed: %v", err)
	}
	if result.NewPinDir != filepath.Join(repoRoot, ".ralph", "pin") {
		t.Fatalf("unexpected new pin dir: %s", result.NewPinDir)
	}
	if _, err := os.Stat(filepath.Join(result.NewPinDir, "implementation_queue.md")); err != nil {
		t.Fatalf("expected queue at new location: %v", err)
	}
	if _, err := os.Stat(filepath.Join(oldPin, "implementation_queue.md")); !os.IsNotExist(err) {
		t.Fatalf("expected old queue to be moved")
	}

	payload := readJSONMap(t, configPath)
	pathsValue, ok := payload["paths"].(map[string]any)
	if !ok {
		t.Fatalf("expected paths map in config")
	}
	if got := pathsValue["pin_dir"]; got != filepath.Join(".ralph", "pin") {
		t.Fatalf("expected pin_dir .ralph/pin, got %#v", got)
	}

	files := pin.ResolveFiles(result.NewPinDir, repoRoot)
	if err := pin.ValidatePin(files); err != nil {
		t.Fatalf("pin validation failed: %v", err)
	}
}

func TestRunKeepsExistingPin(t *testing.T) {
	repoRoot := t.TempDir()
	newPin := filepath.Join(repoRoot, ".ralph", "pin")
	writeMinimalPin(t, repoRoot, newPin)

	configPath := filepath.Join(repoRoot, ".ralph", "ralph.json")
	result, err := Run(repoRoot, configPath)
	if err != nil {
		t.Fatalf("Run failed: %v", err)
	}
	if len(result.Moved) != 0 {
		t.Fatalf("expected no moved entries, got %v", result.Moved)
	}
	if _, err := os.Stat(configPath); err != nil {
		t.Fatalf("expected config written: %v", err)
	}
}

func writeMinimalPin(t *testing.T, repoRoot string, pinDir string) {
	t.Helper()
	if err := os.MkdirAll(pinDir, 0o700); err != nil {
		t.Fatalf("mkdir pin: %v", err)
	}
	queue := filepath.Join(pinDir, "implementation_queue.md")
	done := filepath.Join(pinDir, "implementation_done.md")
	lookup := filepath.Join(pinDir, "lookup_table.md")
	readme := filepath.Join(pinDir, "README.md")

	writeFile(t, queue, "## Queue\n- [ ] IDFQ-0001 [code]: Test item. (README.md)\n  - Evidence: test\n  - Plan: test\n\n## Blocked\n\n## Parking Lot\n")
	writeFile(t, done, "## Done\n")
	writeFile(t, lookup, "")
	writeFile(t, readme, "Pin docs\n")

	promptDir := filepath.Join(repoRoot, "ralph_legacy")
	if err := os.MkdirAll(promptDir, 0o700); err != nil {
		t.Fatalf("mkdir prompt dir: %v", err)
	}
	writeFile(t, filepath.Join(promptDir, "prompt.md"), "")
}

func writeFile(t *testing.T, path string, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
}

func readJSONMap(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal(data, &payload); err != nil {
		t.Fatalf("unmarshal json: %v", err)
	}
	return payload
}
