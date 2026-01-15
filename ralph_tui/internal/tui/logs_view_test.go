// Package tui provides tests for the logs view rendering.
package tui

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLogsViewRefreshRendersContent(t *testing.T) {
	tmpDir := t.TempDir()
	logPath := filepath.Join(tmpDir, "ralph_tui.log")
	if err := os.WriteFile(logPath, []byte("first\nsecond\n"), 0o600); err != nil {
		t.Fatalf("write log file: %v", err)
	}

	view := newLogsView(logPath)
	view.Refresh([]string{"loop line"}, []string{"spec line"})
	content := view.renderContent()

	if !strings.Contains(content, "second") {
		t.Fatalf("expected debug log content, got %q", content)
	}
	if !strings.Contains(content, "loop line") {
		t.Fatalf("expected loop content, got %q", content)
	}
	if !strings.Contains(content, "spec line") {
		t.Fatalf("expected specs content, got %q", content)
	}
}

func TestLogsViewFormattedRendersJSONL(t *testing.T) {
	tmpDir := t.TempDir()
	logPath := filepath.Join(tmpDir, "ralph_tui.log")
	entry := logEntry{
		Timestamp: "2026-01-14T12:34:56Z",
		Level:     "info",
		Message:   "tui.start",
		Fields: map[string]any{
			"screen": "logs",
			"count":  2,
		},
	}
	payload, err := json.Marshal(entry)
	if err != nil {
		t.Fatalf("marshal log entry: %v", err)
	}
	if err := os.WriteFile(logPath, append(payload, '\n'), 0o600); err != nil {
		t.Fatalf("write log file: %v", err)
	}

	view := newLogsView(logPath)
	view.Refresh(nil, nil)
	view.ToggleFormat()
	content := view.renderContent()

	if strings.Contains(content, string(payload)) {
		t.Fatalf("expected formatted output instead of raw JSONL, got %q", content)
	}
	if !strings.Contains(content, "2026-01-14 12:34:56Z") {
		t.Fatalf("expected formatted timestamp, got %q", content)
	}
	if !strings.Contains(content, "INFO") || !strings.Contains(content, "tui.start") {
		t.Fatalf("expected formatted level and message, got %q", content)
	}
	if !strings.Contains(content, "count=2") || !strings.Contains(content, "screen=logs") {
		t.Fatalf("expected formatted fields, got %q", content)
	}
}

func TestTailFileLines(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	missingPath := filepath.Join(tmpDir, "missing.log")

	makeLines := func(count int) []string {
		lines := make([]string, count)
		for i := 0; i < count; i++ {
			lines[i] = fmt.Sprintf("line-%d", i)
		}
		return lines
	}

	tests := []struct {
		name    string
		content string
		limit   int
		want    []string
		path    string
	}{
		{
			name:  "missing file",
			path:  missingPath,
			limit: 5,
			want:  []string{},
		},
		{
			name:    "empty file",
			content: "",
			limit:   5,
			want:    []string{},
		},
		{
			name:    "fewer than limit",
			content: "a\nb\n",
			limit:   200,
			want:    []string{"a", "b"},
		},
		{
			name:    "exactly limit",
			content: strings.Join(makeLines(5), "\n") + "\n",
			limit:   5,
			want:    makeLines(5),
		},
		{
			name:    "no trailing newline",
			content: "a\nb",
			limit:   10,
			want:    []string{"a", "b"},
		},
		{
			name:    "large file tail",
			content: strings.Join(makeLines(20000), "\n") + "\n",
			limit:   200,
			want:    makeLines(20000)[19800:],
		},
		{
			name:    "long line boundary",
			content: "start\n" + strings.Repeat("x", 70*1024) + "\nend1\nend2\n",
			limit:   2,
			want:    []string{"end1", "end2"},
		},
	}

	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			path := test.path
			if path == "" {
				path = filepath.Join(tmpDir, test.name+".log")
			}
			if test.path == "" {
				if err := os.WriteFile(path, []byte(test.content), 0o600); err != nil {
					t.Fatalf("write log file: %v", err)
				}
			}

			got, err := tailFileLines(path, test.limit)
			if err != nil {
				t.Fatalf("tailFileLines: %v", err)
			}
			if strings.Join(got, "\n") != strings.Join(test.want, "\n") {
				t.Fatalf("unexpected lines: got %q want %q", got, test.want)
			}
		})
	}
}

func TestLogsViewRefreshSuppressesRedundantViewportUpdates(t *testing.T) {
	tmpDir := t.TempDir()
	logPath := filepath.Join(tmpDir, "ralph_tui.log")
	if err := os.WriteFile(logPath, []byte("first\nsecond\n"), 0o600); err != nil {
		t.Fatalf("write log file: %v", err)
	}

	view := newLogsView(logPath)
	view.Refresh([]string{"loop line"}, []string{"spec line"})
	if view.viewportSetContentCalls != 1 {
		t.Fatalf("expected one viewport update, got %d", view.viewportSetContentCalls)
	}

	view.Refresh([]string{"loop line"}, []string{"spec line"})
	if view.viewportSetContentCalls != 1 {
		t.Fatalf("expected cached viewport update, got %d", view.viewportSetContentCalls)
	}

	handle, err := os.OpenFile(logPath, os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		t.Fatalf("open log file: %v", err)
	}
	if _, err := handle.WriteString("third\n"); err != nil {
		_ = handle.Close()
		t.Fatalf("append log file: %v", err)
	}
	if err := handle.Close(); err != nil {
		t.Fatalf("close log file: %v", err)
	}

	view.Refresh([]string{"loop line"}, []string{"spec line"})
	if view.viewportSetContentCalls != 2 {
		t.Fatalf("expected refreshed viewport update, got %d", view.viewportSetContentCalls)
	}
}
