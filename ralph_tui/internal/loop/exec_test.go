// Package loop provides tests for command execution helpers.
package loop

import (
	"context"
	"os/exec"
	"strings"
	"testing"
)

type captureLogger struct {
	lines []string
}

func (c *captureLogger) WriteLine(line string) {
	c.lines = append(c.lines, line)
}

func TestRunCommandPreservesStdin(t *testing.T) {
	ctx := context.Background()
	cmd := exec.Command("cat")
	cmd.Stdin = strings.NewReader("hello\n")

	logger := &captureLogger{}
	if err := RunCommand(ctx, cmd, nil, logger); err != nil {
		t.Fatalf("RunCommand failed: %v", err)
	}
	if len(logger.lines) != 1 {
		t.Fatalf("expected 1 line, got %d", len(logger.lines))
	}
	if logger.lines[0] != "hello" {
		t.Fatalf("expected stdout to be hello, got %q", logger.lines[0])
	}
}
