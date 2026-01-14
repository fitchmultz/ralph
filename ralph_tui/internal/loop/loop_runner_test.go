// Package loop provides tests for loop behaviors.
// Entrypoint: go test ./...
package loop

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

type bufferLogger struct {
	lines []string
}

func (b *bufferLogger) WriteLine(line string) {
	b.lines = append(b.lines, line)
}

func TestRunnerStopsOnEmptyQueue(t *testing.T) {
	repoRoot := t.TempDir()
	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	if err := os.MkdirAll(pinDir, 0o700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}

	queue := filepath.Join(pinDir, "implementation_queue.md")
	done := filepath.Join(pinDir, "implementation_done.md")
	lookup := filepath.Join(pinDir, "lookup_table.md")
	readme := filepath.Join(pinDir, "README.md")

	writeFile(t, queue, "## Queue\n\n## Blocked\n\n## Parking Lot\n")
	writeFile(t, done, "## Done\n")
	writeFile(t, lookup, "")
	writeFile(t, readme, "")

	logger := &bufferLogger{}
	runner, err := NewRunner(Options{
		RepoRoot:          repoRoot,
		PinDir:            pinDir,
		PromptPath:        "",
		SupervisorPrompt:  "",
		Runner:            "codex",
		SleepSeconds:      0,
		MaxIterations:     0,
		MaxStalled:        0,
		MaxRepairAttempts: 0,
		OnlyTags:          []string{},
		Once:              true,
		RequireMain:       false,
		AutoCommit:        false,
		AutoPush:          false,
		RedactionMode:     redaction.ModeSecretsOnly,
		Logger:            logger,
	})
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	if err := runner.Run(context.Background()); err != nil {
		t.Fatalf("Run failed: %v", err)
	}
}

func writeFile(t *testing.T, path string, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
}
