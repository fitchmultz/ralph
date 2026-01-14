// Package tui provides render contract tests for layout bounds.
// Entrypoint: go test ./...
package tui

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestRenderContract(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)

	sizes := []struct {
		w int
		h int
	}{
		{w: 48, h: 12},
		{w: 60, h: 20},
		{w: 80, h: 24},
		{w: 100, h: 40},
		{w: 120, h: 50},
	}

	screens := []screen{
		screenDashboard,
		screenRunLoop,
		screenBuildSpecs,
		screenPin,
		screenConfig,
		screenLogs,
		screenHelp,
	}

	for _, size := range sizes {
		for _, showAll := range []bool{false, true} {
			for _, navFocused := range []bool{true, false} {
				for _, screen := range screens {
					name := fmt.Sprintf("w%dxh%d-help%t-focus%t-%s", size.w, size.h, showAll, navFocused, screenLabel(screen))
					t.Run(name, func(t *testing.T) {
						m := newModel(cfg, locs)
						m.screen = screen
						m.navFocused = navFocused
						m.help.ShowAll = showAll
						m.applyFocus()
						m.width = size.w
						m.height = size.h
						m.relayout()
						assertRenderFits(t, m, size.w, size.h)
					})
				}
			}
		}
	}
}

func assertRenderFits(t *testing.T, m model, w, h int) {
	t.Helper()
	out := m.View()
	lines := strings.Split(strings.TrimRight(out, "\n"), "\n")
	footer := m.help.View(m.helpKeyMap())
	footerH := lipgloss.Height(footer)
	for i, line := range lines {
		if lipgloss.Width(line) > w {
			t.Fatalf("line %d exceeds width %d: %d (bodyH=%d footerH=%d)", i+1, w, lipgloss.Width(line), m.layout.bodyHeight, footerH)
		}
	}
	if len(lines) > h {
		t.Fatalf("output exceeds height %d: %d (bodyH=%d footerH=%d)", h, len(lines), m.layout.bodyHeight, footerH)
	}
}

func screenLabel(value screen) string {
	switch value {
	case screenDashboard:
		return "dashboard"
	case screenRunLoop:
		return "runloop"
	case screenBuildSpecs:
		return "buildspecs"
	case screenPin:
		return "pin"
	case screenConfig:
		return "config"
	case screenLogs:
		return "logs"
	case screenHelp:
		return "help"
	default:
		return "unknown"
	}
}
