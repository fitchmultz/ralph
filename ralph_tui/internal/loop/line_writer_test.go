// Package loop provides tests for lineWriter streaming behavior.
package loop

import (
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/streaming"
)

func TestLineWriterSplitsOnCarriageReturn(t *testing.T) {
	logger := &captureLogger{}
	writer := newLineWriter(nil, logger, nil)

	if _, err := writer.Write([]byte("step 1\rstep 2\r")); err != nil {
		t.Fatalf("write: %v", err)
	}

	if len(logger.lines) != 2 {
		t.Fatalf("expected 2 lines, got %d", len(logger.lines))
	}
	if logger.lines[0] != "step 1" || logger.lines[1] != "step 2" {
		t.Fatalf("unexpected lines: %#v", logger.lines)
	}
}

func TestLineWriterFlushesPartialBufferAtLimit(t *testing.T) {
	logger := &captureLogger{}
	writer := newLineWriter(nil, logger, nil)
	payload := strings.Repeat("a", streaming.DefaultMaxBufferedBytes+1)

	if _, err := writer.Write([]byte(payload)); err != nil {
		t.Fatalf("write: %v", err)
	}

	if len(logger.lines) != 1 {
		t.Fatalf("expected 1 line, got %d", len(logger.lines))
	}
	if logger.lines[0] != payload {
		t.Fatalf("unexpected line: %q", logger.lines[0])
	}
}
