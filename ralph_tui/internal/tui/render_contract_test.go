// Package tui provides render contract tests for layout bounds.
// Entrypoint: go test ./...
package tui

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

func TestRenderContract(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)

	sizes := renderContractSizes()

	screens := []screen{
		screenDashboard,
		screenRunLoop,
		screenBuildSpecs,
		screenTaskBuilder,
		screenPin,
		screenConfig,
		screenLogs,
		screenHelp,
	}

	for _, size := range sizes {
		for _, showAll := range []bool{false, true} {
			for _, navCollapsed := range []bool{false, true} {
				for _, navFocused := range []bool{true, false} {
					for _, screen := range screens {
						name := fmt.Sprintf("w%dxh%d-help%t-collapse%t-focus%t-%s", size.w, size.h, showAll, navCollapsed, navFocused, screenName(screen))
						t.Run(name, func(t *testing.T) {
							m := newModel(cfg, locs, StartOptions{})
							m.screen = screen
							m.navCollapsed = navCollapsed
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
}

func TestDashboardRenderContractNarrowSizes(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)

	sizes := dashboardNarrowSizes()

	for _, size := range sizes {
		for _, showAll := range []bool{false, true} {
			for _, navCollapsed := range []bool{false, true} {
				name := fmt.Sprintf("dashboard-narrow-w%dxh%d-help%t-collapse%t", size.w, size.h, showAll, navCollapsed)
				t.Run(name, func(t *testing.T) {
					m := newModel(cfg, locs, StartOptions{})
					m.screen = screenDashboard
					m.navCollapsed = navCollapsed
					m.navFocused = false
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

func TestDashboardRepoPanelFitsNarrowSizes(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)

	sizes := dashboardRepoPanelSizes()

	for _, size := range sizes {
		t.Run(fmt.Sprintf("repo-narrow-w%dxh%d", size.w, size.h), func(t *testing.T) {
			m := newModel(cfg, locs, StartOptions{})
			m.screen = screenDashboard
			m.navCollapsed = true
			m.navFocused = false
			m.repoStatus = repoStatusResult{
				Snapshot: repoStatusSnapshot{
					Branch:         "very-long-branch-name-to-truncate",
					ShortHead:      "abcdef123456",
					StatusSummary:  "## main...origin/main [ahead 12, behind 3]",
					DirtyCount:     42,
					AheadCount:     12,
					LastCommit:     "abcdef1 This is a deliberately long commit subject line to force truncation",
					LastCommitStat: "15 files changed, 123 insertions(+), 45 deletions(-)",
				},
			}
			m.applyFocus()
			m.width = size.w
			m.height = size.h
			m.relayout()
			assertRenderFits(t, m, size.w, size.h)
		})
	}
}

func assertRenderFits(t *testing.T, m model, w, h int) {
	t.Helper()
	out := m.View()
	lines := strings.Split(strings.TrimRight(out, "\n"), "\n")
	footer := strings.TrimRight(m.help.View(m.helpKeyMap()), "\n")
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

func TestBorderContract(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	sizes := renderContractSizes()
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
			for _, navCollapsed := range []bool{false, true} {
				for _, navFocused := range []bool{true, false} {
					for _, screen := range screens {
						name := fmt.Sprintf("border-w%dxh%d-help%t-collapse%t-focus%t-%s", size.w, size.h, showAll, navCollapsed, navFocused, screenName(screen))
						t.Run(name, func(t *testing.T) {
							m := newModel(cfg, locs, StartOptions{})
							m.screen = screen
							m.navCollapsed = navCollapsed
							m.navFocused = navFocused
							m.help.ShowAll = showAll
							m.applyFocus()
							m.width = size.w
							m.height = size.h
							m.relayout()
							out := m.View()
							assertBodyBordersIntact(t, out, size.w, m.layout.bodyHeight)
						})
					}
				}
			}
		}
	}
}

func assertBodyBordersIntact(t *testing.T, out string, w int, bodyH int) {
	t.Helper()
	if bodyH <= 0 {
		return
	}
	lines := strings.Split(strings.TrimRight(out, "\n"), "\n")
	if len(lines) < bodyH {
		bodyH = len(lines)
	}
	for i := 0; i < bodyH; i++ {
		line := lines[i]
		if lipgloss.Width(line) != w {
			t.Fatalf("body line %d width mismatch: expected %d got %d", i+1, w, lipgloss.Width(line))
		}
		plain := ansi.Strip(line)
		if plain == "" {
			t.Fatalf("body line %d is empty", i+1)
		}
		runes := []rune(plain)
		first := runes[0]
		last := runes[len(runes)-1]
		switch {
		case i == 0:
			if first != '╭' || last != '╮' {
				t.Fatalf("top border mismatch line %d: %q ... %q (%q)", i+1, first, last, plain)
			}
		case i == bodyH-1:
			if first != '╰' || last != '╯' {
				t.Fatalf("bottom border mismatch line %d: %q ... %q (%q)", i+1, first, last, plain)
			}
		default:
			if first != '│' || last != '│' {
				t.Fatalf("side border mismatch line %d: %q ... %q (%q)", i+1, first, last, plain)
			}
		}
	}
}

func TestHelpScreenMentionsCtrlFocus(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	m := newModel(cfg, locs, StartOptions{})
	m.screen = screenHelp
	view := m.contentView()
	if !strings.Contains(view, "Ctrl+F") {
		t.Fatalf("expected help screen to mention Ctrl+F focus, got %q", view)
	}
}
