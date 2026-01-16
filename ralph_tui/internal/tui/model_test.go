// Package tui provides tests for basic model transitions.
// Entrypoint: go test ./...
package tui

import (
	"strings"
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

func TestConfigScreenRendersFormWithoutInput(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs, StartOptions{})

	updated, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	m = updated.(model)

	m.switchScreen(screenConfig, true)
	if m.configView == nil {
		t.Fatalf("expected config view to be initialized")
	}

	view := m.configView.View()
	if view == "" {
		t.Fatalf("expected config view to render, got empty view")
	}
	if !strings.Contains(view, "UI Theme") {
		t.Fatalf("expected config view to include UI Theme field")
	}
}

func TestFocusToggleIgnoredWhileTyping(t *testing.T) {
	cases := []struct {
		name  string
		setup func(t *testing.T, m *model)
	}{
		{
			name: "config editor",
			setup: func(t *testing.T, m *model) {
				t.Helper()
				m.switchScreen(screenConfig, true)
				if m.configView == nil {
					t.Fatalf("config view missing")
				}
				m.configView.form.NextField()
				m.configView.form.NextField()
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
				t.Fatalf("expected content focus before ctrl+f handling")
			}

			updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlF})
			next := updated.(model)
			if next.navFocused {
				t.Fatalf("expected ctrl+f to stay with form, focus toggled to nav")
			}
		})
	}
}

func TestFocusToggleWorksWhenNotTyping(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base
	m.navFocused = false
	m.navCollapsed = false
	m.applyFocus()

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlF})
	next := updated.(model)
	if !next.navFocused {
		t.Fatalf("expected ctrl+f to toggle focus to nav")
	}
}

func TestTypingBlocksGlobalShortcutsInSpecsUserFocus(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base
	m.switchScreen(screenBuildSpecs, true)
	if m.specsView == nil {
		t.Fatalf("specs view missing")
	}
	m.specsView.openUserFocusEditor()

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)
	if m.searchActive {
		t.Fatalf("expected search to remain inactive while typing")
	}

	updated, _ = m.Update(runeKey('e'))
	m = updated.(model)
	if m.screen != screenBuildSpecs {
		t.Fatalf("expected screen to remain build specs while typing")
	}

	updated, _ = m.Update(runeKey('q'))
	m = updated.(model)
	if m.shuttingDown {
		t.Fatalf("expected 'q' to be ignored while typing")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlQ})
	m = updated.(model)
	if !m.shuttingDown {
		t.Fatalf("expected ctrl+q to quit while typing")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	m = updated.(model)
	if !m.shuttingDown {
		t.Fatalf("expected ctrl+c to quit while typing")
	}
}

func TestTypingBlocksGlobalSearchInConfigEditorInput(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base
	m.switchScreen(screenConfig, true)
	if m.configView == nil {
		t.Fatalf("config view missing")
	}
	m.configView.form.NextField()
	m.configView.form.NextField()

	before := m.configView.data.UITheme
	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)
	if m.searchActive {
		t.Fatalf("expected search to remain inactive while typing")
	}
	if m.configView.data.UITheme != before {
		t.Fatalf("expected config input to ignore ctrl+k while typing")
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

func TestSearchRoutesSelectionKeysToNav(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	updated, _ := base.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	m := updated.(model)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	if !m.searchActive {
		t.Fatalf("expected search to be active")
	}
	if !m.navPanelFocusedEffective() {
		t.Fatalf("expected nav panel to be focused during nav search")
	}
	if !strings.Contains(m.searchInput.Prompt, "Search:") {
		t.Fatalf("expected search prompt to include Search")
	}

	startIndex := m.nav.Index()
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)
	if m.nav.Index() != startIndex+1 {
		t.Fatalf("expected nav selection to move down")
	}
	if m.searchInput.Value() != "" {
		t.Fatalf("expected search input to remain unchanged")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnd})
	m = updated.(model)
	if m.nav.Index() != len(navigationItems())-1 {
		t.Fatalf("expected nav selection to move to end")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyHome})
	m = updated.(model)
	if m.nav.Index() != 0 {
		t.Fatalf("expected nav selection to move to start")
	}
}

func TestSearchEscRestoresNavSelection(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base

	m.nav.Select(3)
	priorIndex := m.nav.Index()

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)
	if m.nav.Index() == priorIndex {
		t.Fatalf("expected nav selection to change during search")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	m = updated.(model)
	if m.searchActive {
		t.Fatalf("expected search to be inactive after esc")
	}
	if m.nav.Index() != priorIndex {
		t.Fatalf("expected nav selection to restore after esc")
	}
}

func TestSearchEnterSelectsNavItem(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)
	item, ok := m.nav.SelectedItem().(navItem)
	if !ok {
		t.Fatalf("expected nav item selection")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(model)

	if m.searchActive {
		t.Fatalf("expected search to be inactive after enter")
	}
	if m.screen != item.screen {
		t.Fatalf("expected screen %v after enter, got %v", item.screen, m.screen)
	}
	if m.navFocused {
		t.Fatalf("expected content focus after enter")
	}
}

func runeKey(r rune) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}}
}
