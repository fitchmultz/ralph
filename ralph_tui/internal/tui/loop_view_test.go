// Package tui provides tests for loop view behaviors.
package tui

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
)

func TestLoopStopTransitionsToStopping(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.mode = loopRunning
	cancelled := false
	view.cancel = func() { cancelled = true }

	_ = view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'s'}}, newKeyMap())

	if !cancelled {
		t.Fatalf("expected stop to invoke cancel")
	}
	if view.mode != loopStopping {
		t.Fatalf("expected loopStopping, got %v", view.mode)
	}
	if view.status != "Stopping..." {
		t.Fatalf("expected status to be Stopping..., got %q", view.status)
	}
}

func TestLoopLogBatchIgnoresStaleRun(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.logRunID = 2

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Lines: []string{"stale line"}},
	}, newKeyMap())

	if len(view.LogLines()) != 0 {
		t.Fatalf("expected stale log batch to be ignored")
	}
}

func TestLoopLogBatchAppendsAndCloses(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.logRunID = 1
	view.logCh = make(chan string)

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Lines: []string{"line one", "line two"}},
	}, newKeyMap())

	logLines := view.LogLines()
	if len(logLines) != 2 {
		t.Fatalf("expected 2 log lines, got %d", len(logLines))
	}
	if logLines[0] != "line one" || logLines[1] != "line two" {
		t.Fatalf("unexpected log lines: %#v", logLines)
	}

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Done: true},
	}, newKeyMap())

	if view.logCh != nil {
		t.Fatalf("expected log channel to be cleared after done batch")
	}
}

func TestLoopLogFlushesSmallBatchWhileRunning(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.mode = loopRunning
	view.logRunID = 1

	_ = view.Update(loopLogBatchMsg{
		batch: logBatch{RunID: 1, Lines: []string{"line one"}},
	}, newKeyMap())

	if view.pendingViewportLines != 0 {
		t.Fatalf("expected pending viewport lines to flush, got %d", view.pendingViewportLines)
	}
}

func TestLoopStateUpdatesAndClears(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.stateRunID = 2
	view.stateCh = make(chan loop.State)

	state := loop.State{
		Mode:            loop.ModeOnce,
		Iteration:       1,
		ActiveItemID:    "RQ-9001",
		ActiveItemTitle: "Test item",
	}
	_ = view.Update(loopStateMsg{runID: 2, state: state}, newKeyMap())

	if view.state.ActiveItemID != "RQ-9001" {
		t.Fatalf("expected active item to update, got %q", view.state.ActiveItemID)
	}
	if view.state.Iteration != 1 {
		t.Fatalf("expected iteration 1, got %d", view.state.Iteration)
	}

	_ = view.Update(loopStateMsg{runID: 2, done: true}, newKeyMap())
	if view.stateCh != nil {
		t.Fatalf("expected state channel to clear on done")
	}
}

func TestLoopStateLatestOnlyAfterManyUpdates(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.stateRunID = 1
	stateCh := make(chan loop.State, 1)
	view.stateCh = stateCh
	sink := loopStateSink{ch: stateCh}

	for i := 1; i <= 40; i++ {
		sink.Update(loop.State{
			Mode:            loop.ModeContinuous,
			Iteration:       i,
			ActiveItemID:    fmt.Sprintf("RQ-%04d", i),
			ActiveItemTitle: "Streaming update",
		})
	}

	msg := listenLoopState(stateCh, 1)()
	cmd := view.Update(msg, newKeyMap())

	if view.state.Iteration != 40 {
		t.Fatalf("expected latest iteration 40, got %d", view.state.Iteration)
	}
	if view.state.ActiveItemID != "RQ-0040" {
		t.Fatalf("expected latest active item ID RQ-0040, got %q", view.state.ActiveItemID)
	}
	if cmd == nil {
		t.Fatalf("expected loop state update to resubscribe")
	}
}

func TestLoopStateIgnoresStaleRun(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.stateRunID = 3

	_ = view.Update(loopStateMsg{
		runID: 2,
		state: loop.State{ActiveItemID: "RQ-1"},
	}, newKeyMap())

	if view.state.ActiveItemID != "" {
		t.Fatalf("expected stale loop state to be ignored")
	}
}

func TestLoopRunControlsShowSingleIterationWhenOnce(t *testing.T) {
	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
	view.mode = loopRunning
	view.state = loop.State{Mode: loop.ModeOnce}
	view.overrides.MaxIterations = 9

	controls := view.controlsView()
	if !strings.Contains(controls, "Max iterations: 1") {
		t.Fatalf("expected single-run controls to show max iterations 1, got %q", controls)
	}
}

func TestLoopControlsHideEffortWhenRunnerUnsupported(t *testing.T) {
	cfg := testLoopConfig()
	cfg.Loop.Runner = "opencode"
	view := newLoopView(cfg, paths.Locations{}, newTestKeyMap())

	controls := view.controlsView()

	if strings.Contains(controls, "Force context_builder") {
		t.Fatalf("expected force context_builder controls to be hidden for opencode, got %q", controls)
	}
	if strings.Contains(controls, "mandatory:") {
		t.Fatalf("expected no mandatory label for opencode, got %q", controls)
	}
	if strings.Contains(controls, "p force context_builder") {
		t.Fatalf("expected context builder key hint to be hidden for opencode, got %q", controls)
	}
	if !strings.Contains(controls, "Reasoning effort: n/a") {
		t.Fatalf("expected reasoning effort to be marked as n/a for opencode, got %q", controls)
	}
}

func TestLoopControlsShowEffortWhenRunnerSupported(t *testing.T) {
	cfg := testLoopConfig()
	cfg.Loop.Runner = "codex"
	cfg.Loop.ReasoningEffort = "auto"
	view := newLoopView(cfg, paths.Locations{}, newTestKeyMap())

	controls := view.controlsView()

	if !strings.Contains(controls, "Reasoning effort: auto") {
		t.Fatalf("expected reasoning effort controls for codex, got %q", controls)
	}
	if !strings.Contains(controls, "Force context_builder") {
		t.Fatalf("expected context builder controls for codex, got %q", controls)
	}
	if !strings.Contains(controls, "p force context_builder") {
		t.Fatalf("expected context builder key hint for codex, got %q", controls)
	}
}

func TestLoopStartBlocksOnDirtyRepoPolicyError(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skipf("missing git: %v", err)
	}
	repoRoot := t.TempDir()
	runCmd := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = repoRoot
		if output, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v failed: %v\n%s", args, err, string(output))
		}
	}
	runCmd("init", "-b", "main")
	runCmd("config", "user.email", "test@example.com")
	runCmd("config", "user.name", "Test User")
	if err := os.WriteFile(filepath.Join(repoRoot, "README.md"), []byte("base\n"), 0o600); err != nil {
		t.Fatalf("write readme: %v", err)
	}
	runCmd("add", ".")
	runCmd("commit", "-m", "init")
	if err := os.WriteFile(filepath.Join(repoRoot, "README.md"), []byte("dirty\n"), 0o600); err != nil {
		t.Fatalf("write readme: %v", err)
	}

	cfg := testLoopConfig()
	cfg.Loop.DirtyRepo.StartPolicy = "error"
	cfg.Loop.DirtyRepo.AllowUntracked = true
	view := newLoopView(cfg, paths.Locations{RepoRoot: repoRoot}, newTestKeyMap())

	_ = view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'r'}}, newKeyMap())

	if view.mode != loopIdle {
		t.Fatalf("expected loop to remain idle, got %v", view.mode)
	}
	if view.err == "" {
		t.Fatalf("expected error to be set for dirty repo")
	}
}

func testLoopConfig() config.Config {
	return config.Config{
		Loop: config.LoopConfig{
			SleepSeconds:      0,
			MaxIterations:     0,
			MaxStalled:        0,
			MaxRepairAttempts: 0,
			OnlyTags:          "",
			RequireMain:       false,
			DirtyRepo: config.DirtyRepoConfig{
				StartPolicy:              "error",
				DuringPolicy:             "quarantine",
				AllowUntracked:           true,
				QuarantineCleanUntracked: false,
			},
		},
		Git: config.GitConfig{
			AutoCommit: false,
			AutoPush:   false,
		},
	}
}
