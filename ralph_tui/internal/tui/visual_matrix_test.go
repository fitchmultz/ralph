// Package tui provides exhaustive visual matrix testing for the TUI.
// Entrypoint: go test ./...
package tui

import (
	"fmt"
	"testing"
)

type visualState struct {
	navCollapsed bool
	navFocused   bool
	showAllHelp  bool
	typing       bool
}

func TestVisualMatrix_AllScreens(t *testing.T) {
	withAsciiColorProfile(t, func() {
		for _, size := range renderContractSizes() {
			for _, state := range visualStates() {
				for _, screen := range navigationScreens() {
					if state.navCollapsed && state.navFocused {
						continue
					}
					if state.typing && !screenSupportsTyping(screen) {
						continue
					}
					if state.typing && state.navFocused {
						continue
					}
					name := fmt.Sprintf("%s-w%dxh%d-collapse%t-focus%t-help%t-typing%t",
						screenName(screen), size.w, size.h, state.navCollapsed, state.navFocused, state.showAllHelp, state.typing)
					t.Run(name, func(t *testing.T) {
						m := newVisualModel(t, screen, state)
						m.width = size.w
						m.height = size.h
						m.relayout()
						assertRenderFits(t, m, size.w, size.h)
						assertContainsLines(t, m.View(), expectedTitle(screen, state))
					})
				}
			}
		}
	})
}

func TestVisualSnapshots_StableScreens(t *testing.T) {
	withAsciiColorProfile(t, func() {
		const w, h = 80, 24
		for _, state := range visualStates() {
			if state.typing {
				continue
			}
			if state.navCollapsed && state.navFocused {
				continue
			}
			for _, screen := range stableSnapshotScreens() {
				name := fmt.Sprintf("%s-w%dxh%d-collapse%t-focus%t-help%t",
					screenName(screen), w, h, state.navCollapsed, state.navFocused, state.showAllHelp)
				t.Run(name, func(t *testing.T) {
					m := newVisualModel(t, screen, state)
					m.width = w
					m.height = h
					m.relayout()
					assertSnapshot(t, "visuals", name, m.View())
				})
			}
		}
	})
}

func visualStates() []visualState {
	return []visualState{
		{navCollapsed: false, navFocused: true, showAllHelp: false, typing: false},
		{navCollapsed: false, navFocused: false, showAllHelp: false, typing: false},
		{navCollapsed: true, navFocused: false, showAllHelp: false, typing: false},
		{navCollapsed: false, navFocused: true, showAllHelp: true, typing: false},
		{navCollapsed: false, navFocused: false, showAllHelp: true, typing: false},
		{navCollapsed: true, navFocused: false, showAllHelp: true, typing: false},
		{navCollapsed: false, navFocused: false, showAllHelp: false, typing: true},
		{navCollapsed: true, navFocused: false, showAllHelp: false, typing: true},
	}
}

func navigationScreens() []screen {
	return []screen{
		screenDashboard,
		screenRunLoop,
		screenBuildSpecs,
		screenTaskBuilder,
		screenPin,
		screenConfig,
		screenLogs,
		screenHelp,
	}
}

func stableSnapshotScreens() []screen {
	return []screen{
		screenDashboard,
		screenPin,
		screenConfig,
		screenHelp,
	}
}

func screenSupportsTyping(s screen) bool {
	switch s {
	case screenRunLoop, screenBuildSpecs, screenTaskBuilder, screenPin, screenConfig:
		return true
	default:
		return false
	}
}

func screenTitle(s screen) string {
	switch s {
	case screenDashboard:
		return "Dashboard"
	case screenRunLoop:
		return "Run Loop"
	case screenBuildSpecs:
		return "Build Specs"
	case screenTaskBuilder:
		return "Task Builder"
	case screenPin:
		return "Pin"
	case screenConfig:
		return "Config"
	case screenLogs:
		return "Logs"
	case screenHelp:
		return "Help"
	default:
		return ""
	}
}

func expectedTitle(s screen, state visualState) string {
	if s == screenPin && state.typing {
		return "Block item"
	}
	return screenTitle(s)
}

func newVisualModel(t *testing.T, target screen, state visualState) model {
	t.Helper()
	base, _, _ := newHermeticModel(t)
	m := base
	m.screen = target
	m.navCollapsed = state.navCollapsed
	m.navFocused = state.navFocused
	m.help.ShowAll = state.showAllHelp
	if state.typing {
		m.navFocused = false
		applyTypingState(t, &m)
	}
	m.applyFocus()
	return m
}

func applyTypingState(t *testing.T, m *model) {
	t.Helper()
	switch m.screen {
	case screenConfig:
		if m.configView == nil || m.configView.form == nil {
			t.Fatalf("config view missing")
		}
		m.configView.form.NextField()
		m.configView.form.NextField()
	case screenRunLoop:
		if m.loopView == nil {
			t.Fatalf("loop view missing")
		}
		m.loopView.beginEdit()
	case screenBuildSpecs:
		if m.specsView == nil {
			t.Fatalf("specs view missing")
		}
		m.specsView.editUserFocus = true
	case screenTaskBuilder:
		if m.taskBuilderView == nil || m.taskBuilderView.form == nil {
			t.Fatalf("task builder view missing")
		}
		m.taskBuilderView.form.NextField()
	case screenPin:
		if m.pinView == nil {
			t.Fatalf("pin view missing")
		}
		if err := m.pinView.reload(); err != nil {
			t.Fatalf("reload pin view: %v", err)
		}
		m.pinView.startBlock()
	}
	if !m.isTyping() {
		t.Fatalf("expected typing mode for screen %s", screenName(m.screen))
	}
}
