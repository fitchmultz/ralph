// Package tui provides integration tests for background routing of async view messages.
package tui

import (
	"fmt"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

func TestModelDriver_BackgroundAsyncUpdatesNotDropped(t *testing.T) {
	m, _, _ := newHermeticModel(t)
	driver := newModelDriver(t, m)
	driver.AssertScreen(screenDashboard)

	item := pin.QueueItem{
		Header: "- [ ] RQ-0100 [code]: Background update",
		Lines:  []string{"- [ ] RQ-0100 [code]: Background update"},
		ID:     "RQ-0100",
	}
	if driver.m.pinView == nil {
		t.Fatalf("expected pin view to be initialized")
	}
	if driver.m.specsView == nil {
		t.Fatalf("expected specs view to be initialized")
	}

	driver.Send(pinReloadMsg{items: []pin.QueueItem{item}})
	driver.Send(specsPreviewMsg{preview: "Preview content", effective: true, auto: true})

	if len(driver.m.pinView.items) != 1 {
		t.Fatalf("expected 1 pin item, got %d", len(driver.m.pinView.items))
	}
	if driver.m.pinView.items[0].ID != item.ID {
		t.Fatalf("expected pin item ID %q, got %q", item.ID, driver.m.pinView.items[0].ID)
	}
	if driver.m.specsView.preview != "Preview content" {
		t.Fatalf("expected specs preview to update, got %q", driver.m.specsView.preview)
	}
}

func TestModelDriver_MidRunSwitchesDoNotDropAsyncMessages(t *testing.T) {
	t.Run("specs", func(t *testing.T) {
		m, _, _ := newHermeticModel(t)
		driver := newModelDriver(t, m)
		driver.SelectScreen(screenBuildSpecs)
		driver.KeyRunes("r")

		if driver.m.specsView == nil {
			t.Fatalf("expected specs view to be initialized")
		}
		if !driver.m.specsView.running {
			t.Fatalf("expected specs run to be marked running")
		}
		runID := driver.m.specsView.logRunID
		if runID == 0 {
			t.Fatalf("expected non-zero run ID")
		}

		driver.SelectScreen(screenDashboard)
		driver.Send(specsBuildResultMsg{diffStat: "1 file changed", effective: true})
		if driver.m.specsView.pendingResult == nil {
			t.Fatalf("expected pending build result when logs still open")
		}
		driver.Send(specsLogBatchMsg{batch: logBatch{RunID: runID, Lines: []string{"line one"}}})
		driver.Send(specsLogBatchMsg{batch: logBatch{RunID: runID, Lines: []string{"line two"}, Done: true}})

		if driver.m.specsView.running {
			t.Fatalf("expected specs run to be marked stopped")
		}
		if driver.m.specsView.logCh != nil {
			t.Fatalf("expected specs log channel to be cleared")
		}
		if driver.m.specsView.pendingResult != nil {
			t.Fatalf("expected pending result to be cleared after completion")
		}
		if !strings.Contains(driver.m.specsView.lastRunOutput, "line two") {
			t.Fatalf("expected run output to include log lines")
		}
		if !strings.Contains(driver.m.specsView.status, "Pin validation OK") {
			t.Fatalf("expected status to report pin validation OK, got %q", driver.m.specsView.status)
		}
	})

	t.Run("loop", func(t *testing.T) {
		m, _, _ := newHermeticModel(t)
		driver := newModelDriver(t, m)
		driver.SelectScreen(screenRunLoop)
		driver.KeyRunes("c")

		if driver.m.loopView == nil {
			t.Fatalf("expected loop view to be initialized")
		}
		if driver.m.loopView.mode != loopRunning {
			t.Fatalf("expected loop to be running")
		}
		runID := driver.m.loopView.logRunID
		if runID == 0 {
			t.Fatalf("expected non-zero run ID")
		}

		driver.KeyRunes("s")
		if driver.m.loopView.mode != loopStopping {
			t.Fatalf("expected loop to be stopping after stop key")
		}

		driver.SelectScreen(screenDashboard)
		for i := 1; i <= 40; i++ {
			driver.Send(loopStateMsg{
				runID: runID,
				state: loop.State{
					Mode:            loop.ModeContinuous,
					Iteration:       i,
					ActiveItemID:    fmt.Sprintf("RQ-%04d", i),
					ActiveItemTitle: "Background update",
				},
			})
		}
		driver.Send(loopLogBatchMsg{batch: logBatch{RunID: runID, Lines: []string{"loop line one"}}})
		driver.Send(loopLogBatchMsg{batch: logBatch{RunID: runID, Lines: []string{"loop line two"}, Done: true}})
		driver.Send(loopResultMsg{})

		if driver.m.loopView.mode != loopIdle {
			t.Fatalf("expected loop to be idle after result")
		}
		if driver.m.loopView.cancel != nil {
			t.Fatalf("expected loop cancel to be cleared")
		}
		if driver.m.loopView.logCh != nil {
			t.Fatalf("expected loop log channel to be cleared")
		}
		if driver.m.loopView.state.Iteration != 40 {
			t.Fatalf("expected latest iteration 40, got %d", driver.m.loopView.state.Iteration)
		}
		if driver.m.loopView.state.ActiveItemID != "RQ-0040" {
			t.Fatalf("expected latest active item ID RQ-0040, got %q", driver.m.loopView.state.ActiveItemID)
		}
		if !strings.Contains(strings.Join(driver.m.loopView.LogLines(), "\n"), "loop line two") {
			t.Fatalf("expected loop logs to include log lines")
		}
		if driver.m.loopView.status != "Stopped" {
			t.Fatalf("expected loop status to be Stopped, got %q", driver.m.loopView.status)
		}
	})
}
