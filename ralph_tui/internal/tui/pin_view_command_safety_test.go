// Package tui provides tests for pin view command safety.
package tui

import (
	"os"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestPinCommandsBlockedWhileLoopRunning(t *testing.T) {
	keys := newTestKeyMap()
	cases := []struct {
		name string
		key  string
	}{
		{name: "validate", key: "v"},
		{name: "edit queue", key: "e"},
		{name: "move checked", key: "m"},
		{name: "block item", key: "b"},
		{name: "toggle checked", key: "x"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			_, locs, cfg := newHermeticModel(t)
			view, err := newPinView(cfg, locs)
			if err != nil {
				t.Fatalf("newPinView failed: %v", err)
			}
			if err := view.reload(); err != nil {
				t.Fatalf("reload pin view: %v", err)
			}

			before, err := os.ReadFile(view.files.QueuePath)
			if err != nil {
				t.Fatalf("read queue: %v", err)
			}

			cmd := view.Update(keyMsg(tc.key), keys, loopRunning)
			if cmd != nil {
				t.Fatalf("expected no command for blocked key %q", tc.key)
			}
			if view.mode != pinModeTable {
				t.Fatalf("expected mode to stay table, got %v", view.mode)
			}
			if view.err != "" {
				t.Fatalf("expected no error, got %q", view.err)
			}
			if view.status != "Pin updates disabled while loop is running." {
				t.Fatalf("expected blocked status, got %q", view.status)
			}

			after, err := os.ReadFile(view.files.QueuePath)
			if err != nil {
				t.Fatalf("read queue: %v", err)
			}
			if string(before) != string(after) {
				t.Fatalf("expected queue file unchanged for key %q", tc.key)
			}
		})
	}
}

func TestPinBlockFormAbortsWhenLoopRunning(t *testing.T) {
	keys := newTestKeyMap()
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}
	if err := view.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}
	view.startBlock()
	if view.mode != pinModeBlockForm {
		t.Fatalf("expected block form mode, got %v", view.mode)
	}

	_ = view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x")}, keys, loopRunning)

	if view.mode != pinModeTable {
		t.Fatalf("expected block form to abort while running")
	}
	if view.blockForm != nil {
		t.Fatalf("expected block form to be cleared")
	}
	if view.status != "Pin updates disabled while loop is running." {
		t.Fatalf("expected blocked status, got %q", view.status)
	}
}

func keyMsg(key string) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
}
