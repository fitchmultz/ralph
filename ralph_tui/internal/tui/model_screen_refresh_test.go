// Package tui provides tests for screen-entry refresh behavior.
package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestScreenEntryRefreshTriggersPinReload(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	if m.pinView == nil {
		t.Fatalf("expected pin view to be initialized")
	}

	cmds := m.switchScreen(screenPin, true)
	if !m.pinView.loading {
		t.Fatalf("expected pin reload to start on entry")
	}
	if len(cmds) == 0 {
		t.Fatalf("expected pin reload command on entry")
	}

	for _, cmd := range cmds {
		if cmd == nil {
			continue
		}
		msg := cmd()
		if msg == nil {
			continue
		}
		updated, _ := m.Update(msg)
		m = updated.(model)
	}

	if len(m.pinView.items) == 0 {
		t.Fatalf("expected pin items to load on entry refresh")
	}
}

func TestScreenEntryRefreshTriggersSpecsPreview(t *testing.T) {
	m, _, cfg := newHermeticModel(t)
	if m.specsView == nil {
		t.Fatalf("expected specs view to be initialized")
	}

	promptPath := filepath.Join(cfg.Paths.PinDir, "specs_builder.md")
	if err := os.WriteFile(promptPath, []byte("Updated prompt.\n"), 0o644); err != nil {
		t.Fatalf("write prompt file: %v", err)
	}

	m.specsView.previewLoading = false
	m.specsView.previewDirty = false

	cmds := m.switchScreen(screenBuildSpecs, true)
	if len(cmds) == 0 {
		t.Fatalf("expected specs preview refresh command on entry")
	}
	if !m.specsView.previewLoading {
		t.Fatalf("expected specs preview to start refreshing on entry")
	}
}

func TestScreenEntryRefreshUpdatesLogsView(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	if m.logsView == nil || m.loopView == nil {
		t.Fatalf("expected logs and loop views to be initialized")
	}

	before := m.logsView.viewportSetContentCalls
	m.loopView.logBuf.AppendLines([]string{"loop refresh line"})

	_ = m.switchScreen(screenLogs, true)

	if m.logsView.viewportSetContentCalls <= before {
		t.Fatalf("expected logs view to refresh on entry")
	}
	if !strings.Contains(m.logsView.lastRenderedContent, "loop refresh line") {
		t.Fatalf("expected refreshed logs content to include new loop line")
	}
}
