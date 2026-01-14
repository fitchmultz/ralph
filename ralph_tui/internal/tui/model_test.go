// Package tui provides tests for basic model transitions.
// Entrypoint: go test ./...
package tui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestModelScreenTransition(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs)
	m.nav.Select(4)
	m.navFocused = true

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	next := updated.(model)

	if next.screen != screenConfig {
		t.Fatalf("expected screenConfig, got %v", next.screen)
	}
	if next.navFocused {
		t.Fatalf("expected content focus after selection")
	}
}

func TestConfigReloadBumpsRefreshGeneration(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs)
	before := m.refreshGen

	cfg.UI.RefreshSeconds = cfg.UI.RefreshSeconds + 1
	updated, _ := m.Update(configReloadMsg{cfg: cfg})
	next := updated.(model)

	if next.refreshGen != before+1 {
		t.Fatalf("expected refreshGen to increment, got %d", next.refreshGen)
	}
}
