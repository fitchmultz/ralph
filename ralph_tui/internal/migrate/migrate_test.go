// Package migrate provides tests for the Ralph pin migration flow.
// Entrypoint: go test ./...
package migrate

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

func TestRunInitializesPinAndUpdatesConfig(t *testing.T) {
	repoRoot := t.TempDir()
	configPath := filepath.Join(repoRoot, ".ralph", "ralph.json")
	result, err := Run(repoRoot, configPath)
	if err != nil {
		t.Fatalf("Run failed: %v", err)
	}
	if result.PinDir != filepath.Join(repoRoot, ".ralph", "pin") {
		t.Fatalf("unexpected pin dir: %s", result.PinDir)
	}
	if _, err := os.Stat(filepath.Join(result.PinDir, "implementation_queue.md")); err != nil {
		t.Fatalf("expected queue at pin dir: %v", err)
	}

	payload := readJSONMap(t, configPath)
	pathsValue, ok := payload["paths"].(map[string]any)
	if !ok {
		t.Fatalf("expected paths map in config")
	}
	if got := pathsValue["pin_dir"]; got != filepath.Join(".ralph", "pin") {
		t.Fatalf("expected pin_dir .ralph/pin, got %#v", got)
	}
	if got := payload["project_type"]; got != string(project.TypeCode) {
		t.Fatalf("expected project_type code, got %#v", got)
	}

	files := pin.ResolveFiles(result.PinDir)
	if _, err := os.Stat(files.SpecsBuilderCodePath); err != nil {
		t.Fatalf("expected specs_builder after migrate: %v", err)
	}
	if _, err := os.Stat(files.SpecsBuilderDocsPath); err != nil {
		t.Fatalf("expected specs_builder_docs after migrate: %v", err)
	}
	if err := pin.ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("pin validation failed: %v", err)
	}
}

func TestRunInitializesPinAndUpdatesConfigDocs(t *testing.T) {
	repoRoot := t.TempDir()
	configPath := filepath.Join(repoRoot, ".ralph", "ralph.json")
	if err := os.MkdirAll(filepath.Dir(configPath), 0o700); err != nil {
		t.Fatalf("mkdir config dir: %v", err)
	}
	payload := map[string]any{
		"project_type": "docs",
	}
	data, err := json.Marshal(payload)
	if err != nil {
		t.Fatalf("marshal config: %v", err)
	}
	if err := os.WriteFile(configPath, data, 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}

	result, err := Run(repoRoot, configPath)
	if err != nil {
		t.Fatalf("Run failed: %v", err)
	}
	if result.PinDir != filepath.Join(repoRoot, ".ralph", "pin") {
		t.Fatalf("unexpected pin dir: %s", result.PinDir)
	}

	updated := readJSONMap(t, configPath)
	if got := updated["project_type"]; got != string(project.TypeDocs) {
		t.Fatalf("expected project_type docs, got %#v", got)
	}

	files := pin.ResolveFiles(result.PinDir)
	if _, err := os.Stat(files.SpecsBuilderCodePath); err != nil {
		t.Fatalf("expected specs_builder after migrate: %v", err)
	}
	if _, err := os.Stat(files.SpecsBuilderDocsPath); err != nil {
		t.Fatalf("expected specs_builder_docs after migrate: %v", err)
	}
	if err := pin.ValidatePin(files, project.TypeDocs); err != nil {
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
	if len(result.Created) != 0 {
		t.Fatalf("expected no created entries, got %v", result.Created)
	}
	if len(result.Overwritten) != 0 {
		t.Fatalf("expected no overwritten entries, got %v", result.Overwritten)
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
	specsCode := filepath.Join(pinDir, "specs_builder.md")
	specsDocs := filepath.Join(pinDir, "specs_builder_docs.md")

	writeFile(t, queue, "## Queue\n- [ ] RQ-0001 [code]: Test item. (README.md)\n  - Evidence: test\n  - Plan: test\n\n## Blocked\n\n## Parking Lot\n")
	writeFile(t, done, "## Done\n")
	writeFile(t, lookup, "")
	writeFile(t, readme, "Pin docs\n")
	writeFile(t, specsCode, "# Specs builder\n")
	writeFile(t, specsDocs, "# Specs builder docs\n")

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
