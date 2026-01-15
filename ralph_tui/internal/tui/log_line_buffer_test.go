// Package tui provides tests for log line buffering.
package tui

import (
	"strings"
	"testing"
)

func TestLogLineBufferAppendAndContent(t *testing.T) {
	buffer := newLogLineBuffer(10, 0)
	buffer.AppendLines([]string{"one", "two"})
	buffer.AppendLines([]string{"three"})

	got := buffer.ContentString()
	want := "one\ntwo\nthree"
	if got != want {
		t.Fatalf("content mismatch: got %q want %q", got, want)
	}

	lines := buffer.Lines()
	if strings.Join(lines, "|") != "one|two|three" {
		t.Fatalf("lines mismatch: got %q", lines)
	}
}

func TestLogLineBufferTrimmed(t *testing.T) {
	buffer := newLogLineBuffer(5, 3)
	buffer.AppendLines([]string{"a", "b", "c", "d", "e"})
	buffer.AppendLines([]string{"f"})

	lines := buffer.Lines()
	if len(lines) != 3 {
		t.Fatalf("expected trimmed to 3 lines, got %d", len(lines))
	}
	if strings.Join(lines, ",") != "d,e,f" {
		t.Fatalf("trim result mismatch: %v", lines)
	}

	content := buffer.ContentString()
	if content != "d\ne\nf" {
		t.Fatalf("content trim mismatch: %q", content)
	}
}
