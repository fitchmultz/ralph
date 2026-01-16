// Package tui provides tiny-terminal snapshot tests for TUI screens.
package tui

import (
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
)

func TestTinyTerminalSnapshots(t *testing.T) {
	withAsciiColorProfile(t, func() {
		_, locs, cfg := newHermeticModel(t)
		keys := newTestKeyMap()

		pinView, err := newPinView(cfg, locs)
		if err != nil {
			t.Fatalf("newPinView failed: %v", err)
		}
		loopView := newLoopView(cfg, locs, keys)
		specsView, err := newSpecsView(cfg, locs, keys)
		if err != nil {
			t.Fatalf("newSpecsView failed: %v", err)
		}
		configView, err := newConfigEditor(locs, config.PartialConfig{}, config.PartialConfig{})
		if err != nil {
			t.Fatalf("newConfigEditor failed: %v", err)
		}

		cases := []struct {
			name   string
			width  int
			height int
			render func() string
		}{
			{
				name:   "pin_18x8",
				width:  18,
				height: 8,
				render: func() string {
					pinView.Resize(18, 8)
					return clampToSize(pinView.View(), 18, 8)
				},
			},
			{
				name:   "loop_18x8",
				width:  18,
				height: 8,
				render: func() string {
					loopView.Resize(18, 8)
					return clampToSize(loopView.View(), 18, 8)
				},
			},
			{
				name:   "specs_22x10",
				width:  22,
				height: 10,
				render: func() string {
					specsView.Resize(22, 10)
					return clampToSize(specsView.View(), 22, 10)
				},
			},
			{
				name:   "config_18x8",
				width:  18,
				height: 8,
				render: func() string {
					configView.Resize(18, 8)
					return clampToSize(configView.View(), 18, 8)
				},
			},
		}

		for _, tc := range cases {
			caseData := tc
			t.Run(caseData.name, func(t *testing.T) {
				output := caseData.render()
				assertWithinBounds(t, output, caseData.width, caseData.height)
				assertSnapshot(t, "tiny_terminal", caseData.name, output)
			})
		}
	})
}

func assertWithinBounds(t *testing.T, rendered string, width int, height int) {
	t.Helper()
	if rendered == "" {
		return
	}
	lines := strings.Split(rendered, "\n")
	if height > 0 && len(lines) > height {
		t.Fatalf("rendered output exceeds height %d with %d lines", height, len(lines))
	}
	if width > 0 {
		for _, line := range lines {
			if ansi.StringWidth(line) > width {
				t.Fatalf("rendered output exceeds width %d: %q", width, line)
			}
		}
	}
}
