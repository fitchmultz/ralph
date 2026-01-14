// Package specs provides tests for prompt building and innovate resolution.
// Entrypoint: go test ./...
package specs

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestFillPromptReplacements(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "specs_builder.md")
	content := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write template: %v", err)
	}

	prompt, err := FillPrompt(path, true, true)
	if err != nil {
		t.Fatalf("FillPrompt failed: %v", err)
	}
	if !strings.Contains(prompt, "INTERACTIVE MODE ENABLED") {
		t.Fatalf("interactive instructions missing")
	}
	if !strings.Contains(prompt, "AUTOFILL/SCOUT MODE ENABLED") {
		t.Fatalf("innovate instructions missing")
	}
}

func TestFillPromptMissingPlaceholderErrors(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "specs_builder.md")
	content := "AGENTS.md\n"
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write template: %v", err)
	}

	_, err := FillPrompt(path, true, false)
	if err == nil {
		t.Fatalf("expected error for missing interactive placeholder")
	}
}

func TestResolveInnovateAutoEnable(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	queueContent := "## Queue\n\n## Blocked\n\n## Parking Lot\n"
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	effective, err := ResolveInnovate(queuePath, false, false, true)
	if err != nil {
		t.Fatalf("ResolveInnovate failed: %v", err)
	}
	if !effective {
		t.Fatalf("expected innovate auto-enabled when queue empty")
	}
}
