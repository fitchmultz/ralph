// Package tui provides tests for key hint rendering and help alignment.
package tui

import (
	"strings"
	"testing"
)

func TestSpecsViewKeyHintsMatchRunningState(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	keys := newTestKeyMap()
	view, err := newSpecsView(cfg, locs, keys)
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	idleLine := findLineWithPrefix(t, view.optionsView(), "Keys:")
	idleExpected := renderKeyHints("Keys", specsKeyHintBindings(keys, false))
	if idleLine != idleExpected {
		t.Fatalf("expected idle hints %q, got %q", idleExpected, idleLine)
	}

	view.running = true
	runningLine := findLineWithPrefix(t, view.optionsView(), "Keys:")
	runningExpected := renderKeyHints("Keys", specsKeyHintBindings(keys, true))
	if runningLine != runningExpected {
		t.Fatalf("expected running hints %q, got %q", runningExpected, runningLine)
	}
}

func TestLoopViewKeyHintsMatchMode(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	keys := newTestKeyMap()
	view := newLoopView(cfg, locs, keys)
	view.overrides.Runner = "codex"

	idleLine := findLineWithPrefix(t, view.controlsView(), "Keys:")
	idleExpected := renderKeyHints("Keys", loopKeyHintBindings(keys, view.mode, true))
	if idleLine != idleExpected {
		t.Fatalf("expected idle hints %q, got %q", idleExpected, idleLine)
	}

	view.mode = loopRunning
	runningLine := findLineWithPrefix(t, view.controlsView(), "Keys:")
	runningExpected := renderKeyHints("Keys", loopKeyHintBindings(keys, view.mode, true))
	if runningLine != runningExpected {
		t.Fatalf("expected running hints %q, got %q", runningExpected, runningLine)
	}
}

func TestScreenKeyMapReflectsSpecsRunningState(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base
	m.switchScreen(screenBuildSpecs, true)
	if m.specsView == nil {
		t.Fatalf("specs view missing")
	}
	m.specsView.running = true

	rendered := renderKeyHints("Keys", m.screenKeyMap().ShortHelp())
	expected := renderKeyHints("Keys", specsKeyHintBindings(m.keys, true))
	if rendered != expected {
		t.Fatalf("expected specs help hints %q, got %q", expected, rendered)
	}
}

func TestScreenKeyMapReflectsLoopMode(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base
	m.switchScreen(screenRunLoop, true)
	if m.loopView == nil {
		t.Fatalf("loop view missing")
	}
	m.loopView.mode = loopRunning
	m.loopView.overrides.Runner = "codex"

	rendered := renderKeyHints("Keys", m.screenKeyMap().ShortHelp())
	expected := renderKeyHints("Keys", loopKeyHintBindings(m.keys, m.loopMode(), true))
	if rendered != expected {
		t.Fatalf("expected loop help hints %q, got %q", expected, rendered)
	}
}

func findLineWithPrefix(t *testing.T, block string, prefix string) string {
	t.Helper()
	for _, line := range strings.Split(block, "\n") {
		if strings.HasPrefix(line, prefix) {
			return line
		}
	}
	t.Fatalf("expected line with prefix %q", prefix)
	return ""
}
