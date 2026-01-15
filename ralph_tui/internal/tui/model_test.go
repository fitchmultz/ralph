// Package tui provides tests for basic model transitions.
// Entrypoint: go test ./...
package tui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestModelScreenTransition(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs, StartOptions{})
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
	m := newModel(cfg, locs, StartOptions{})
	before := m.refreshGen

	cfg.UI.RefreshSeconds = cfg.UI.RefreshSeconds + 1
	updated, _ := m.Update(configReloadMsg{cfg: cfg})
	next := updated.(model)

	if next.refreshGen != before+1 {
		t.Fatalf("expected refreshGen to increment, got %d", next.refreshGen)
	}
}

func TestTabBypassesGlobalFocusWhenFormActive(t *testing.T) {
	cases := []struct {
		name  string
		setup func(t *testing.T, m *model)
	}{
		{
			name: "config editor",
			setup: func(t *testing.T, m *model) {
				t.Helper()
				m.switchScreen(screenConfig, true)
			},
		},
		{
			name: "loop edit form",
			setup: func(t *testing.T, m *model) {
				t.Helper()
				m.switchScreen(screenRunLoop, true)
				if m.loopView == nil {
					t.Fatalf("loop view missing")
				}
				m.loopView.beginEdit()
			},
		},
		{
			name: "pin block form",
			setup: func(t *testing.T, m *model) {
				t.Helper()
				m.switchScreen(screenPin, true)
				if m.pinView == nil {
					t.Fatalf("pin view missing")
				}
				if err := m.pinView.reload(); err != nil {
					t.Fatalf("reload pin view: %v", err)
				}
				m.pinView.startBlock()
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			base, _, _ := newHermeticModel(t)
			m := base
			tc.setup(t, &m)

			if m.navFocused {
				t.Fatalf("expected content focus before tab handling")
			}

			updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyTab})
			next := updated.(model)
			if next.navFocused {
				t.Fatalf("expected tab to stay with form, focus toggled to nav")
			}

			updated, _ = next.Update(tea.KeyMsg{Type: tea.KeyCtrlF})
			next = updated.(model)
			if !next.navFocused {
				t.Fatalf("expected ctrl+f to toggle focus to nav")
			}
		})
	}
}

func TestNavCollapseForcesContentFocus(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs, StartOptions{})
	m.navFocused = true
	m.navCollapsed = true
	m.applyFocus()

	if m.navFocused {
		t.Fatalf("expected nav focus to clear when collapsed")
	}
}

func TestNavCollapseToggle(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs, StartOptions{})
	m.navFocused = true

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlN})
	next := updated.(model)

	if !next.navCollapsed {
		t.Fatalf("expected nav to be collapsed after toggle")
	}
	if next.navFocused {
		t.Fatalf("expected content focus after collapsing nav")
	}
}
