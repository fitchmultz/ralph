// Package tui provides tests for loop view behaviors.
package tui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
)

func TestLoopStopTransitionsToStopping(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{})
	view.mode = loopRunning
	cancelled := false
	view.cancel = func() { cancelled = true }

	_ = view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'s'}}, newKeyMap())

	if !cancelled {
		t.Fatalf("expected stop to invoke cancel")
	}
	if view.mode != loopStopping {
		t.Fatalf("expected loopStopping, got %v", view.mode)
	}
	if view.status != "Stopping..." {
		t.Fatalf("expected status to be Stopping..., got %q", view.status)
	}
}

func TestLoopLogBatchIgnoresStaleRun(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{})
	view.logRunID = 2

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Lines: []string{"stale line"}},
	}, newKeyMap())

	if len(view.LogLines()) != 0 {
		t.Fatalf("expected stale log batch to be ignored")
	}
}

func TestLoopLogBatchAppendsAndCloses(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{})
	view.logRunID = 1
	view.logCh = make(chan string)

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Lines: []string{"line one", "line two"}},
	}, newKeyMap())

	logLines := view.LogLines()
	if len(logLines) != 2 {
		t.Fatalf("expected 2 log lines, got %d", len(logLines))
	}
	if logLines[0] != "line one" || logLines[1] != "line two" {
		t.Fatalf("unexpected log lines: %#v", logLines)
	}

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Done: true},
	}, newKeyMap())

	if view.logCh != nil {
		t.Fatalf("expected log channel to be cleared after done batch")
	}
}

func testLoopConfig() config.Config {
	return config.Config{
		Loop: config.LoopConfig{
			SleepSeconds:      0,
			MaxIterations:     0,
			MaxStalled:        0,
			MaxRepairAttempts: 0,
			OnlyTags:          "",
			RequireMain:       false,
		},
		Git: config.GitConfig{
			AutoCommit: false,
			AutoPush:   false,
		},
	}
}
