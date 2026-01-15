// Package tui provides tests for basic model transitions.
// Entrypoint: go test ./...
package tui

import (
	"path/filepath"
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

func TestSearchRoutesSelectionKeysToNav(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	updated, _ := base.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	m := updated.(model)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	if !m.searchActive {
		t.Fatalf("expected search to be active")
	}
	if m.searchTarget != searchTargetNav {
		t.Fatalf("expected search target nav, got %v", m.searchTarget)
	}
	if !m.navPanelFocusedEffective() {
		t.Fatalf("expected nav panel to be focused during nav search")
	}
	if !strings.Contains(m.searchInput.Prompt, "Search (Nav):") {
		t.Fatalf("expected search prompt to include nav target")
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

func TestSearchTabTogglesTargetOnPinScreen(t *testing.T) {
	base, _, cfg := newHermeticModel(t)

	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0001 [ui]: First item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"- [ ] RQ-0002 [ui]: Second item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"- [ ] RQ-0003 [ui]: Third item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(cfg.Paths.PinDir, "implementation_queue.md"), queueContent)

	m := base
	m.switchScreen(screenPin, true)
	if m.pinView == nil {
		t.Fatalf("pin view missing")
	}
	if err := m.pinView.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	m = updated.(model)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	if m.searchTarget != searchTargetPin {
		t.Fatalf("expected search target pin, got %v", m.searchTarget)
	}
	if m.navPanelFocusedEffective() {
		t.Fatalf("expected content panel focus during pin search")
	}
	if !strings.Contains(m.searchInput.Prompt, "Search (Pin):") {
		t.Fatalf("expected search prompt to include pin target")
	}

	startCursor := m.pinView.table.Cursor()
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)
	if m.pinView.table.Cursor() == startCursor {
		t.Fatalf("expected pin table cursor to move down")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
	m = updated.(model)
	if m.searchTarget != searchTargetNav {
		t.Fatalf("expected search target nav after tab, got %v", m.searchTarget)
	}
	if !m.navPanelFocusedEffective() {
		t.Fatalf("expected nav panel focus after tab")
	}
	if !strings.Contains(m.searchInput.Prompt, "Search (Nav):") {
		t.Fatalf("expected search prompt to include nav target after tab")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
	m = updated.(model)
	if m.searchTarget != searchTargetPin {
		t.Fatalf("expected search target pin after second tab, got %v", m.searchTarget)
	}
	if m.navPanelFocusedEffective() {
		t.Fatalf("expected content focus after toggling back to pin")
	}
	if !strings.Contains(m.searchInput.Prompt, "Search (Pin):") {
		t.Fatalf("expected search prompt to include pin target after second tab")
	}
}

func TestSearchSelectionKeysAffectOnlyActiveTarget(t *testing.T) {
	base, _, cfg := newHermeticModel(t)

	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0001 [ui]: First item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"- [ ] RQ-0002 [ui]: Second item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(cfg.Paths.PinDir, "implementation_queue.md"), queueContent)

	m := base
	m.switchScreen(screenPin, true)
	if m.pinView == nil {
		t.Fatalf("pin view missing")
	}
	if err := m.pinView.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	m = updated.(model)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	pinCursor := m.pinView.table.Cursor()
	navIndex := m.nav.Index()
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)
	if m.pinView.table.Cursor() == pinCursor {
		t.Fatalf("expected pin cursor to move when pin is target")
	}
	if m.nav.Index() != navIndex {
		t.Fatalf("expected nav selection to remain unchanged when pin is target")
	}

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
	m = updated.(model)

	pinCursor = m.pinView.table.Cursor()
	navIndex = m.nav.Index()
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)
	if m.nav.Index() == navIndex {
		t.Fatalf("expected nav selection to move when nav is target")
	}
	if m.pinView.table.Cursor() != pinCursor {
		t.Fatalf("expected pin cursor to remain unchanged when nav is target")
	}
}

func TestSearchTabDoesNotToggleGlobalFocus(t *testing.T) {
	base, _, _ := newHermeticModel(t)
	m := base

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	priorNavFocused := m.navFocused
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyTab})
	m = updated.(model)

	if !m.searchActive {
		t.Fatalf("expected search to remain active after tab")
	}
	if m.searchTarget != searchTargetNav {
		t.Fatalf("expected search target to remain nav on non-pin screen")
	}
	if m.navFocused != priorNavFocused {
		t.Fatalf("expected global focus to remain unchanged during search")
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
