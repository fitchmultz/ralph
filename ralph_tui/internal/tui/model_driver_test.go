// Package tui provides a model driver harness for integration-style view tests.
package tui

import (
	"errors"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
)

type modelDriver struct {
	t *testing.T
	m model
	w int
	h int
}

func newModelDriver(t *testing.T, m model) *modelDriver {
	t.Helper()
	return &modelDriver{t: t, m: m}
}

func (d *modelDriver) Send(msg tea.Msg) {
	d.t.Helper()
	updated, _ := d.m.Update(msg)
	switch next := updated.(type) {
	case model:
		d.m = next
	case *model:
		d.m = *next
	default:
		d.t.Fatalf("unexpected model type %T", updated)
	}
}

func (d *modelDriver) Resize(w, h int) {
	d.t.Helper()
	d.w = w
	d.h = h
	d.Send(tea.WindowSizeMsg{Width: w, Height: h})
}

func (d *modelDriver) KeyType(k tea.KeyType) {
	d.t.Helper()
	d.Send(tea.KeyMsg{Type: k})
}

func (d *modelDriver) KeyRunes(value string) {
	d.t.Helper()
	d.Send(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(value)})
}

func (d *modelDriver) ToggleFocus() {
	d.t.Helper()
	d.KeyType(tea.KeyCtrlF)
}

func (d *modelDriver) SelectScreen(target screen) {
	d.t.Helper()
	if !d.m.navFocused {
		d.ToggleFocus()
	}
	targetIndex := navIndexForScreen(target)
	if targetIndex < 0 {
		d.t.Fatalf("unknown screen %v", target)
	}
	current := d.m.nav.GlobalIndex()
	switch {
	case current < targetIndex:
		for i := 0; i < targetIndex-current; i++ {
			d.KeyType(tea.KeyDown)
		}
	case current > targetIndex:
		for i := 0; i < current-targetIndex; i++ {
			d.KeyType(tea.KeyUp)
		}
	}
	d.KeyType(tea.KeyEnter)
}

func (d *modelDriver) AssertScreen(target screen) {
	d.t.Helper()
	if d.m.screen != target {
		d.t.Fatalf("expected screen %v, got %v", target, d.m.screen)
	}
}

func (d *modelDriver) AssertViewWithinBounds() {
	d.t.Helper()
	view := strings.TrimRight(d.m.View(), "\n")
	if d.w > 0 {
		for _, line := range strings.Split(view, "\n") {
			if lipgloss.Width(line) > d.w {
				d.t.Fatalf("line exceeds width %d: %q", d.w, line)
			}
		}
	}
	if d.h > 0 {
		if lipgloss.Height(view) > d.h {
			d.t.Fatalf("view exceeds height %d", d.h)
		}
	}
}

func TestModelDriver_NavigationResizeKeyFlows(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	driver := newModelDriver(t, m)

	driver.Resize(120, 40)
	driver.AssertViewWithinBounds()

	driver.SelectScreen(screenBuildSpecs)
	driver.AssertScreen(screenBuildSpecs)
	driver.AssertViewWithinBounds()

	driver.KeyRunes("r")
	driver.AssertViewWithinBounds()

	driver.KeyRunes("e")
	driver.AssertScreen(screenConfig)
	driver.AssertViewWithinBounds()

	driver.ToggleFocus()
	driver.SelectScreen(screenRunLoop)
	driver.AssertScreen(screenRunLoop)
	driver.KeyRunes("r")
	driver.KeyRunes("s")
	driver.AssertViewWithinBounds()

	driver.ToggleFocus()
	driver.SelectScreen(screenLogs)
	driver.AssertScreen(screenLogs)
	driver.KeyRunes("f")
	driver.AssertViewWithinBounds()

	for _, size := range []struct {
		w int
		h int
	}{
		{w: 80, h: 20},
		{w: 50, h: 12},
		{w: 32, h: 8},
		{w: 24, h: 6},
	} {
		driver.Resize(size.w, size.h)
		driver.AssertViewWithinBounds()
	}
}

func TestModelDriver_AllScreens_RenderWithinBounds(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	driver := newModelDriver(t, m)
	driver.Resize(40, 10)

	for _, item := range navigationItems() {
		driver.SelectScreen(item.screen)
		driver.AssertScreen(item.screen)
		driver.AssertViewWithinBounds()
	}
}

func TestDashboardRepoStatusDegradesWithoutGit(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	driver := newModelDriver(t, m)
	driver.Resize(40, 10)
	driver.SelectScreen(screenDashboard)

	driver.Send(repoStatusMsg{err: errors.New("git missing")})

	view := driver.m.contentView()
	if !strings.Contains(view, "Repo:") {
		t.Fatalf("expected repo section in dashboard, got %q", view)
	}
	if !strings.Contains(view, "git missing") {
		t.Fatalf("expected repo status to show git missing, got %q", view)
	}
}

func TestDashboardFixupKeyStartsRunAndUpdatesStatus(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	driver := newModelDriver(t, m)
	driver.SelectScreen(screenDashboard)

	driver.KeyRunes("f")

	if !driver.m.fixup.running {
		t.Fatalf("expected fixup to be running")
	}
	if driver.m.fixupLogCh == nil {
		t.Fatalf("expected fixup log channel to be initialized")
	}
	runID := driver.m.fixupLogRunID

	if driver.m.loopView == nil {
		t.Fatalf("expected loop view to be initialized")
	}

	driver.Send(fixupLogBatchMsg{batch: logBatch{RunID: runID, Lines: []string{"fixup line"}}})
	if !strings.Contains(strings.Join(driver.m.loopView.LogLines(), "\n"), "fixup line") {
		t.Fatalf("expected fixup log line to be captured in loop logs")
	}

	driver.Send(fixupLogBatchMsg{batch: logBatch{RunID: runID, Done: true}})
	if driver.m.fixupLogCh != nil {
		t.Fatalf("expected fixup log channel to be cleared on completion")
	}

	result := loop.FixupResult{
		ScannedBlocked: 2,
		Eligible:       1,
		RequeuedIDs:    []string{"RQ-0002"},
		FailedIDs:      []string{"RQ-0003"},
	}
	driver.Send(fixupResultMsg{runID: runID, result: result, finishedAt: time.Now()})

	if driver.m.fixup.running {
		t.Fatalf("expected fixup to be marked stopped")
	}
	view := driver.m.contentView()
	if !strings.Contains(view, "Fixup: Scanned 2 | Eligible 1 | Requeued 1 | Skipped 0 | Failed 1") {
		t.Fatalf("expected dashboard to report fixup summary, got %q", view)
	}
}
