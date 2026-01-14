// Package tui provides tests for basic model transitions.
// Entrypoint: go test ./...
package tui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/config"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/paths"
)

func TestModelScreenTransition(t *testing.T) {
	locs, err := paths.Resolve("")
	if err != nil {
		t.Fatalf("resolve paths: %v", err)
	}
	cfg, err := config.LoadFromLocations(config.LoadOptions{Locations: locs})
	if err != nil {
		t.Fatalf("load config: %v", err)
	}
	m := newModel(cfg, locs)
	m.nav.Select(4)
	m.navFocused = true

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	next := updated.(model)

	if next.screen != screenConfig {
		t.Fatalf("expected screenConfig, got %v", next.screen)
	}
}
