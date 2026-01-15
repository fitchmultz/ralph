// Package tui provides the Run Loop screen.
// Entrypoint: loopView.
package tui

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

type loopView struct {
	cfg                  config.Config
	locations            paths.Locations
	viewport             viewport.Model
	overrides            loopOverrides
	mode                 loopMode
	status               string
	err                  string
	outputErr            string
	logBuf               logLineBuffer
	cancel               context.CancelFunc
	editForm             *huh.Form
	editData             loopFormData
	guardForm            *huh.Form
	guardData            loopGuardData
	guardRunOnce         bool
	logCh                chan string
	logRunID             int
	stateCh              chan loop.State
	stateRunID           int
	state                loop.State
	logger               *tuiLogger
	output               *outputFileWriter
	width                int
	height               int
	pendingViewportLines int
	lastViewportVersion  uint64
	lastViewportFlush    time.Time
}

type loopOverrides struct {
	SleepSeconds        int
	MaxIterations       int
	MaxStalled          int
	MaxRepairAttempts   int
	OnlyTags            string
	RequireMain         bool
	AutoCommit          bool
	AutoPush            bool
	Runner              string
	RunnerArgs          []string
	ReasoningEffort     string
	ForceContextBuilder bool
	DirtyRepoStart      string
	DirtyRepoDuring     string
	AllowUntracked      bool
	QuarantineClean     bool
}

type loopMode int

const (
	loopIdle loopMode = iota
	loopRunning
	loopStopping
	loopEditing
	loopGuarding
)

type loopGuardAction string

const (
	guardActionProceed    loopGuardAction = "proceed"
	guardActionCancel     loopGuardAction = "cancel"
	guardActionShowStatus loopGuardAction = "show_status"
	guardActionShowDiff   loopGuardAction = "show_diff"
	guardActionCommitPin  loopGuardAction = "commit_pin"
	guardActionStash      loopGuardAction = "stash"
)

type loopGuardData struct {
	Action loopGuardAction
}

type loopResultMsg struct {
	err error
}

type loopLogBatchMsg struct {
	batch logBatch
}

type loopStateMsg struct {
	runID int
	state loop.State
	done  bool
}

type loopLogger struct {
	write func(string)
}

func (l loopLogger) WriteLine(line string) {
	if l.write != nil {
		l.write(line)
	}
}

func newLoopView(cfg config.Config, locations paths.Locations) *loopView {
	vp := viewport.New(80, 20)
	return &loopView{
		cfg:       cfg,
		locations: locations,
		viewport:  vp,
		logBuf:    newLogLineBuffer(2000, 1500),
		overrides: loopOverrides{
			SleepSeconds:        cfg.Loop.SleepSeconds,
			MaxIterations:       cfg.Loop.MaxIterations,
			MaxStalled:          cfg.Loop.MaxStalled,
			MaxRepairAttempts:   cfg.Loop.MaxRepairAttempts,
			OnlyTags:            cfg.Loop.OnlyTags,
			RequireMain:         cfg.Loop.RequireMain,
			AutoCommit:          cfg.Git.AutoCommit,
			AutoPush:            cfg.Git.AutoPush,
			Runner:              cfg.Loop.Runner,
			RunnerArgs:          cfg.Loop.RunnerArgs,
			ReasoningEffort:     cfg.Loop.ReasoningEffort,
			ForceContextBuilder: false,
			DirtyRepoStart:      cfg.Loop.DirtyRepo.StartPolicy,
			DirtyRepoDuring:     cfg.Loop.DirtyRepo.DuringPolicy,
			AllowUntracked:      cfg.Loop.DirtyRepo.AllowUntracked,
			QuarantineClean:     cfg.Loop.DirtyRepo.QuarantineCleanUntracked,
		},
		mode:   loopIdle,
		status: "Idle",
		state:  loop.State{Mode: loop.ModeIdle},
	}
}

func (l *loopView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
	if l.mode == loopGuarding && l.guardForm != nil {
		model, cmd := l.guardForm.Update(msg)
		if form, ok := model.(*huh.Form); ok {
			l.guardForm = form
		}
		if l.guardForm.State == huh.StateCompleted {
			return l.applyGuardAction()
		}
		if l.guardForm.State == huh.StateAborted {
			l.guardForm = nil
			l.mode = loopIdle
			l.status = "Run cancelled"
		}
		return cmd
	}
	if l.mode == loopEditing && l.editForm != nil {
		model, cmd := l.editForm.Update(msg)
		if form, ok := model.(*huh.Form); ok {
			l.editForm = form
		}
		if l.editForm.State == huh.StateCompleted {
			if err := l.applyEditData(); err != nil {
				l.err = err.Error()
				l.status = "Edit failed"
			} else {
				l.err = ""
				l.status = "Session overrides updated"
			}
			l.mode = loopIdle
		} else if l.editForm.State == huh.StateAborted {
			l.mode = loopIdle
			l.status = "Edit cancelled"
		}
		return cmd
	}

	switch msg := msg.(type) {
	case loopResultMsg:
		l.mode = loopIdle
		l.state.Mode = loop.ModeIdle
		if msg.err != nil {
			l.err = msg.err.Error()
			l.status = "Stopped with error"
			if l.logger != nil {
				l.logger.Error("loop.stop", map[string]any{"error": msg.err.Error()})
			}
		} else {
			l.err = ""
			l.status = "Stopped"
			if l.logger != nil {
				l.logger.Info("loop.stop", map[string]any{"status": "completed"})
			}
		}
		l.stopPersistingOutput()
		l.cancel = nil
		l.Resize(l.width, l.height)
		return loopRunModeCmd(false)
	case loopStateMsg:
		if msg.runID != l.stateRunID {
			return nil
		}
		if msg.done {
			l.stateCh = nil
			return nil
		}
		l.state = msg.state
		l.Resize(l.width, l.height)
		if l.stateCh != nil {
			return listenLoopState(l.stateCh, l.stateRunID)
		}
		return nil
	case loopLogBatchMsg:
		if msg.batch.RunID != l.logRunID {
			return nil
		}
		if len(msg.batch.Lines) > 0 {
			l.appendLogLines(msg.batch.Lines)
		}
		if msg.batch.Done {
			l.flushLogViewport(l.viewport.AtBottom())
			l.logCh = nil
			l.stopPersistingOutput()
			return nil
		}
		if l.logCh != nil {
			return listenLoopLogs(l.logCh, l.logRunID)
		}
		return nil
	case tea.KeyMsg:
		switch {
		case key.Matches(msg, keys.RunLoopOnce) && l.mode == loopIdle:
			return l.startWithGuard(true)
		case key.Matches(msg, keys.RunLoopContinuous) && l.mode == loopIdle:
			return l.startWithGuard(false)
		case key.Matches(msg, keys.StopLoop) && l.mode == loopRunning:
			l.stop()
			l.mode = loopStopping
			l.status = "Stopping..."
			l.err = ""
			l.appendLogLine(">> [RALPH] Stop requested.")
			if l.logger != nil {
				l.logger.Info("loop.stop.request", map[string]any{"status": l.status})
			}
			return nil
		case key.Matches(msg, keys.EditLoopConfig) && l.mode == loopIdle:
			l.beginEdit()
			return nil
		case key.Matches(msg, keys.ToggleForceContextBuilder) && l.mode != loopEditing:
			l.overrides.ForceContextBuilder = !l.overrides.ForceContextBuilder
			state := yesNo(l.overrides.ForceContextBuilder)
			l.status = fmt.Sprintf("Force context_builder: %s", state)
			if l.mode == loopRunning {
				l.appendLogLine(fmt.Sprintf(">> [RALPH] Force context_builder set to %s (applies next run).", state))
			}
			return nil
		}
	}

	if l.mode != loopEditing {
		updated, cmd := l.viewport.Update(msg)
		l.viewport = updated
		return cmd
	}
	return nil
}

func (l *loopView) StartOnce() tea.Cmd {
	if l == nil || l.mode != loopIdle {
		return nil
	}
	return l.startWithGuard(true)
}

func (l *loopView) HandlesTabNavigation() bool {
	return (l.mode == loopEditing && l.editForm != nil) || (l.mode == loopGuarding && l.guardForm != nil)
}

func (l *loopView) ActiveItemID() string {
	if l == nil {
		return ""
	}
	return l.state.ActiveItemID
}

func (l *loopView) View() string {
	head := "Run Loop"
	status := l.statusLine()
	controls := l.controlsView()
	if l.mode == loopGuarding && l.guardForm != nil {
		return withFinalNewline(head + "\n" + status + "\n\n" + l.guardForm.View())
	}
	if l.mode == loopEditing && l.editForm != nil {
		return withFinalNewline(head + "\n" + status + "\n\n" + l.editForm.View())
	}
	state := l.stateView()
	logs := l.viewport.View()
	body := head + "\n" + status
	if state != "" {
		body += "\n" + state
	}
	return withFinalNewline(body + "\n\n" + controls + "\n\n" + logs)
}

func (l *loopView) statusLine() string {
	if l.err != "" {
		return fmt.Sprintf("Error: %s | Force context_builder: %s", l.err, yesNo(l.overrides.ForceContextBuilder))
	}
	if l.outputErr != "" {
		return l.status + " | Persist error: " + l.outputErr + " | Force context_builder: " + yesNo(l.overrides.ForceContextBuilder)
	}
	return l.status + " | Force context_builder: " + yesNo(l.overrides.ForceContextBuilder)
}

func (l *loopView) controlsView() string {
	if l.mode == loopRunning || l.mode == loopStopping {
		return l.runControlsView()
	}
	autoTarget := l.autoTargetEffort()
	effortResult := runnerargs.ApplyReasoningEffortWithAutoTarget(
		l.overrides.Runner,
		l.overrides.RunnerArgs,
		l.overrides.ReasoningEffort,
		autoTarget,
	)
	effectiveLabel := runnerargs.DisplayEffortResult(effortResult)
	mandatory := l.overrides.ForceContextBuilder || effortResult.Effective == "low" || effortResult.Effective == "off"
	lines := []string{
		fmt.Sprintf("Sleep seconds: %d", l.overrides.SleepSeconds),
		fmt.Sprintf("Max iterations: %d", l.overrides.MaxIterations),
		fmt.Sprintf("Max stalled: %d", l.overrides.MaxStalled),
		fmt.Sprintf("Max repair attempts: %d", l.overrides.MaxRepairAttempts),
		fmt.Sprintf("Only tags: %s", l.overrides.OnlyTags),
		fmt.Sprintf("Require main: %s", yesNo(l.overrides.RequireMain)),
		fmt.Sprintf("Auto commit: %s", yesNo(l.overrides.AutoCommit)),
		fmt.Sprintf("Auto push: %s", yesNo(l.overrides.AutoPush)),
		fmt.Sprintf("Dirty repo start/during: %s/%s", l.overrides.DirtyRepoStart, l.overrides.DirtyRepoDuring),
		fmt.Sprintf("Allow untracked: %s | Quarantine clean: %s", yesNo(l.overrides.AllowUntracked), yesNo(l.overrides.QuarantineClean)),
		fmt.Sprintf("Runner: %s", l.overrides.Runner),
		fmt.Sprintf("Runner args: %d", len(l.overrides.RunnerArgs)),
		fmt.Sprintf("Reasoning effort: %s (effective: %s)", runnerargs.DisplayEffort(l.overrides.ReasoningEffort), effectiveLabel),
		fmt.Sprintf("Force context_builder: %s (mandatory: %s)", yesNo(l.overrides.ForceContextBuilder), yesNo(mandatory)),
		"Keys: r run once | c continuous | s stop | e edit overrides | p force ctx builder | shift+p pin | shift+l logs",
	}
	return strings.Join(lines, "\n")
}

func (l *loopView) autoTargetEffort() string {
	normalized := runnerargs.NormalizeEffort(l.overrides.ReasoningEffort)
	if normalized != "" && normalized != "auto" {
		return ""
	}
	onlyTags, err := parseOnlyTags(l.overrides.OnlyTags)
	if err != nil {
		return ""
	}
	files := pin.ResolveFiles(l.cfg.Paths.PinDir)
	item, err := loop.FirstUncheckedItem(files.QueuePath, onlyTags)
	if err != nil || item == nil {
		return ""
	}
	if strings.Contains(item.Header, "[P1]") {
		return "high"
	}
	return ""
}

func (l *loopView) runControlsView() string {
	lines := []string{
		fmt.Sprintf("Runner: %s (%s)", l.overrides.Runner, runnerargs.DisplayEffort(l.overrides.ReasoningEffort)),
		fmt.Sprintf("Sleep: %ds | Max iterations: %s | Max stalled: %d", l.overrides.SleepSeconds, l.iterationLimitLabel(), l.overrides.MaxStalled),
		fmt.Sprintf("Only tags: %s | Require main: %s | Auto commit/push: %s/%s", l.overrides.OnlyTags, yesNo(l.overrides.RequireMain), yesNo(l.overrides.AutoCommit), yesNo(l.overrides.AutoPush)),
		fmt.Sprintf("Dirty repo start/during: %s/%s | Allow untracked: %s | Quarantine clean: %s", l.overrides.DirtyRepoStart, l.overrides.DirtyRepoDuring, yesNo(l.overrides.AllowUntracked), yesNo(l.overrides.QuarantineClean)),
		"Keys: s stop | e edit overrides | p force ctx builder | shift+p pin | shift+l logs",
	}
	return strings.Join(lines, "\n")
}

func (l *loopView) iterationLimitLabel() string {
	if l.state.Mode == loop.ModeOnce {
		return "1"
	}
	return iterationLimitLabel(l.overrides.MaxIterations)
}

func iterationLimitLabel(limit int) string {
	if limit <= 0 {
		return "unlimited"
	}
	return fmt.Sprintf("%d", limit)
}

func (l *loopView) stateView() string {
	if l.state.ActiveItemID == "" && l.state.Iteration == 0 {
		return ""
	}
	lines := []string{}
	if l.state.ActiveItemID != "" {
		title := strings.TrimSpace(l.state.ActiveItemTitle)
		if title != "" {
			lines = append(lines, fmt.Sprintf("Active item: %s — %s", l.state.ActiveItemID, title))
		} else {
			lines = append(lines, fmt.Sprintf("Active item: %s", l.state.ActiveItemID))
		}
	}
	if l.state.Iteration > 0 {
		maxLabel := l.iterationLimitLabel()
		if maxLabel != "unlimited" {
			lines = append(lines, fmt.Sprintf("Iteration: %d of %s (%s)", l.state.Iteration, maxLabel, l.state.Mode))
		} else {
			lines = append(lines, fmt.Sprintf("Iteration: %d (%s)", l.state.Iteration, l.state.Mode))
		}
	}
	if l.state.LastFailureStage != "" || l.state.LastFailureMessage != "" {
		stage := strings.TrimSpace(l.state.LastFailureStage)
		message := strings.TrimSpace(l.state.LastFailureMessage)
		if stage == "" {
			stage = "unknown"
		}
		if message == "" {
			lines = append(lines, fmt.Sprintf("Last failure: %s", stage))
		} else {
			lines = append(lines, fmt.Sprintf("Last failure: %s — %s", stage, message))
		}
	}
	if len(lines) == 0 {
		return ""
	}
	return strings.Join(lines, "\n")
}

func (l *loopView) startWithGuard(runOnce bool) tea.Cmd {
	ctx := context.Background()
	policy, err := loop.ParseDirtyRepoPolicy(l.overrides.DirtyRepoStart)
	if err != nil {
		l.err = err.Error()
		l.status = "Start failed"
		return nil
	}
	if policy == "" {
		policy = loop.DirtyRepoPolicyError
	}
	status, err := loop.StatusDetails(ctx, l.locations.RepoRoot)
	if err != nil {
		l.err = err.Error()
		l.status = "Start failed"
		return nil
	}
	if status.IsClean(l.overrides.AllowUntracked) {
		return l.startRun(runOnce)
	}
	if policy == loop.DirtyRepoPolicyError {
		l.err = "Dirty repo detected; start policy blocks the loop."
		l.status = "Start blocked"
		l.appendGitStatusSummary(ctx)
		return nil
	}
	l.beginDirtyGuard(runOnce, status)
	return nil
}

func (l *loopView) startRun(runOnce bool) tea.Cmd {
	onlyTags, err := parseOnlyTags(l.overrides.OnlyTags)
	if err != nil {
		l.err = err.Error()
		l.status = "Start failed"
		return nil
	}

	l.err = ""
	l.outputErr = ""
	l.status = "Running"
	l.mode = loopRunning
	if runOnce {
		l.state = loop.State{Mode: loop.ModeOnce, Iteration: 0}
	} else {
		l.state = loop.State{Mode: loop.ModeContinuous, Iteration: 0}
	}
	l.startPersistingOutput()
	l.lastViewportFlush = time.Time{}
	l.Resize(l.width, l.height)

	ctx, cancel := context.WithCancel(context.Background())
	l.cancel = cancel

	logCh := newLogChannel()
	l.logCh = logCh
	l.logRunID++
	runID := l.logRunID
	stateCh := make(chan loop.State, 1)
	l.stateCh = stateCh
	l.stateRunID++
	stateRunID := l.stateRunID
	if l.logger != nil {
		applied := runnerargs.ApplyReasoningEffort(l.overrides.Runner, l.overrides.RunnerArgs, l.overrides.ReasoningEffort)
		l.logger.Info("loop.start", map[string]any{
			"mode":                  loopModeLabel(runOnce),
			"sleep_seconds":         l.overrides.SleepSeconds,
			"max_iterations":        l.overrides.MaxIterations,
			"max_stalled":           l.overrides.MaxStalled,
			"max_repair":            l.overrides.MaxRepairAttempts,
			"only_tags":             l.overrides.OnlyTags,
			"require_main":          l.overrides.RequireMain,
			"auto_commit":           l.overrides.AutoCommit,
			"auto_push":             l.overrides.AutoPush,
			"dirty_start_policy":    l.overrides.DirtyRepoStart,
			"dirty_during_policy":   l.overrides.DirtyRepoDuring,
			"allow_untracked":       l.overrides.AllowUntracked,
			"quarantine_clean":      l.overrides.QuarantineClean,
			"runner":                l.overrides.Runner,
			"runner_args_count":     len(applied.Args),
			"reasoning_effort":      runnerargs.DisplayEffort(l.overrides.ReasoningEffort),
			"force_context_builder": l.overrides.ForceContextBuilder,
		})
	}

	runCmd := func() tea.Msg {
		logger := loopLogger{
			write: func(line string) {
				sendLineBestEffort(logCh, line)
			},
		}
		stateSink := loopStateSink{
			ch: stateCh,
		}
		runner, err := loop.NewRunner(loop.Options{
			RepoRoot:            l.locations.RepoRoot,
			PinDir:              l.cfg.Paths.PinDir,
			PromptPath:          "",
			SupervisorPrompt:    "",
			Runner:              l.overrides.Runner,
			RunnerArgs:          runnerargs.ApplyReasoningEffort(l.overrides.Runner, l.overrides.RunnerArgs, l.overrides.ReasoningEffort).Args,
			ReasoningEffort:     l.overrides.ReasoningEffort,
			SleepSeconds:        l.overrides.SleepSeconds,
			MaxIterations:       l.overrides.MaxIterations,
			MaxStalled:          l.overrides.MaxStalled,
			MaxRepairAttempts:   l.overrides.MaxRepairAttempts,
			OnlyTags:            onlyTags,
			Once:                runOnce,
			RequireMain:         l.overrides.RequireMain,
			AutoCommit:          l.overrides.AutoCommit,
			AutoPush:            l.overrides.AutoPush,
			DirtyRepoStart:      loop.DirtyRepoPolicy(l.overrides.DirtyRepoStart),
			DirtyRepoDuring:     loop.DirtyRepoPolicy(l.overrides.DirtyRepoDuring),
			AllowUntracked:      l.overrides.AllowUntracked,
			QuarantineClean:     l.overrides.QuarantineClean,
			ForceContextBuilder: l.overrides.ForceContextBuilder,
			RedactionMode:       l.cfg.Logging.RedactionMode,
			Logger:              logger,
			StateSink:           stateSink,
		})
		if err != nil {
			close(logCh)
			close(stateCh)
			return loopResultMsg{err: err}
		}
		if err := runner.Run(ctx); err != nil {
			close(logCh)
			close(stateCh)
			return loopResultMsg{err: err}
		}
		close(logCh)
		close(stateCh)
		return loopResultMsg{}
	}

	return tea.Batch(runCmd, listenLoopLogs(logCh, runID), listenLoopState(stateCh, stateRunID), loopRunModeCmd(true))
}

func (l *loopView) beginDirtyGuard(runOnce bool, status loop.GitStatus) {
	l.guardRunOnce = runOnce
	l.guardData = loopGuardData{Action: guardActionShowStatus}
	l.guardForm = l.buildGuardForm()
	l.mode = loopGuarding
	l.err = ""
	trackedCount := len(status.TrackedEntries())
	untrackedCount := len(status.UntrackedEntries())
	l.status = fmt.Sprintf("Dirty repo: %d tracked, %d untracked", trackedCount, untrackedCount)
	l.Resize(l.width, l.height)
}

func (l *loopView) buildGuardForm() *huh.Form {
	return huh.NewForm(
		huh.NewGroup(
			huh.NewSelect[loopGuardAction]().
				Title("Dirty repo detected. Choose an action.").
				Options(
					huh.NewOption("Show git status", guardActionShowStatus),
					huh.NewOption("Show git diff --stat", guardActionShowDiff),
					huh.NewOption("Commit pin-only changes", guardActionCommitPin),
					huh.NewOption("Stash changes", guardActionStash),
					huh.NewOption("Proceed (start anyway)", guardActionProceed),
					huh.NewOption("Cancel", guardActionCancel),
				).
				Value(&l.guardData.Action),
		),
	).WithShowHelp(false)
}

func (l *loopView) applyGuardAction() tea.Cmd {
	action := l.guardData.Action
	runOnce := l.guardRunOnce
	l.guardForm = nil
	l.guardData = loopGuardData{}

	switch action {
	case guardActionProceed:
		l.mode = loopIdle
		return l.startRun(runOnce)
	case guardActionCancel:
		l.mode = loopIdle
		l.status = "Run cancelled"
		l.err = ""
		return nil
	case guardActionShowStatus:
		l.appendGitStatusSummary(context.Background())
		return l.refreshDirtyGuard(runOnce)
	case guardActionShowDiff:
		l.appendGitDiffSummary(context.Background())
		return l.refreshDirtyGuard(runOnce)
	case guardActionCommitPin:
		committed, err := l.commitPinOnly(context.Background())
		if err != nil {
			l.err = err.Error()
			l.status = "Pin commit failed"
			return l.refreshDirtyGuard(runOnce)
		}
		if committed {
			l.status = "Committed pin changes"
		} else {
			l.status = "No pin changes to commit"
		}
		return l.refreshDirtyGuard(runOnce)
	case guardActionStash:
		if err := loop.Stash(context.Background(), l.locations.RepoRoot, true, "ralph pre-run stash"); err != nil {
			l.err = err.Error()
			l.status = "Stash failed"
			return l.refreshDirtyGuard(runOnce)
		}
		l.status = "Stashed changes"
		return l.refreshDirtyGuard(runOnce)
	default:
		l.mode = loopIdle
		l.status = "Run cancelled"
		return nil
	}
}

func (l *loopView) refreshDirtyGuard(runOnce bool) tea.Cmd {
	status, err := loop.StatusDetails(context.Background(), l.locations.RepoRoot)
	if err != nil {
		l.err = err.Error()
		l.status = "Guard failed"
		l.mode = loopIdle
		return nil
	}
	if status.IsClean(l.overrides.AllowUntracked) {
		l.mode = loopIdle
		return l.startRun(runOnce)
	}
	l.beginDirtyGuard(runOnce, status)
	return nil
}

func (l *loopView) appendGitStatusSummary(ctx context.Context) {
	summary, err := loop.StatusSummary(ctx, l.locations.RepoRoot)
	if err != nil {
		l.err = err.Error()
		l.status = "Status check failed"
		return
	}
	l.appendLogLine(">> [RALPH] git status -sb")
	for _, line := range strings.Split(summary, "\n") {
		l.appendLogLine(">> [RALPH] " + line)
	}
}

func (l *loopView) appendGitDiffSummary(ctx context.Context) {
	stat, err := loop.DiffStat(ctx, l.locations.RepoRoot)
	if err != nil {
		l.err = err.Error()
		l.status = "Diff check failed"
		return
	}
	l.appendLogLine(">> [RALPH] git diff --stat")
	for _, line := range strings.Split(stat, "\n") {
		l.appendLogLine(">> [RALPH] " + line)
	}
	names, err := loop.DiffNameOnly(ctx, l.locations.RepoRoot)
	if err != nil {
		l.err = err.Error()
		l.status = "Diff check failed"
		return
	}
	if len(names) > 0 {
		l.appendLogLine(">> [RALPH] git diff --name-only")
		for _, name := range names {
			l.appendLogLine(">> [RALPH] " + name)
		}
	}
}

func (l *loopView) commitPinOnly(ctx context.Context) (bool, error) {
	files := pin.ResolveFiles(l.cfg.Paths.PinDir)
	return loop.CommitPinChanges(ctx, l.locations.RepoRoot, files, "chore: commit pin changes (pre-loop)")
}

func (l *loopView) stop() {
	if l.cancel != nil {
		l.cancel()
	}
}

func (l *loopView) beginEdit() {
	l.editData = loopFormDataFromOverrides(l.overrides)
	l.editForm = l.buildEditForm()
	l.mode = loopEditing
	l.Resize(l.width, l.height)
}

func (l *loopView) buildEditForm() *huh.Form {
	return huh.NewForm(
		huh.NewGroup(
			huh.NewInput().Title("Sleep Seconds").Value(&l.editData.SleepSeconds).Validate(nonNegativeInt("loop.sleep_seconds")),
			huh.NewInput().Title("Max Iterations").Value(&l.editData.MaxIterations).Validate(nonNegativeInt("loop.max_iterations")),
			huh.NewInput().Title("Max Stalled").Value(&l.editData.MaxStalled).Validate(nonNegativeInt("loop.max_stalled")),
			huh.NewInput().Title("Max Repair Attempts").Value(&l.editData.MaxRepairAttempts).Validate(nonNegativeInt("loop.max_repair_attempts")),
		),
		huh.NewGroup(
			huh.NewInput().Title("Only Tags").Value(&l.editData.OnlyTags),
			huh.NewConfirm().Title("Require Main Branch").Value(&l.editData.RequireMain),
		),
		huh.NewGroup(
			huh.NewConfirm().Title("Auto Commit").Value(&l.editData.AutoCommit),
			huh.NewConfirm().Title("Auto Push").Value(&l.editData.AutoPush),
		),
		huh.NewGroup(
			huh.NewSelect[string]().
				Title("Loop Runner").
				Options(
					huh.NewOption("codex", "codex"),
					huh.NewOption("opencode", "opencode"),
				).
				Value(&l.editData.Runner),
			huh.NewText().Title("Loop Runner Args (one per line)").Value(&l.editData.RunnerArgs).Lines(3),
			huh.NewSelect[string]().
				Title("Loop Reasoning Effort").
				Options(
					huh.NewOption("auto", "auto"),
					huh.NewOption("low", "low"),
					huh.NewOption("medium", "medium"),
					huh.NewOption("high", "high"),
					huh.NewOption("off", "off"),
				).
				Value(&l.editData.ReasoningEffort),
			huh.NewConfirm().Title("Force context_builder").Value(&l.editData.ForceContextBuilder),
		),
	).WithShowHelp(false)
}

func (l *loopView) applyEditData() error {
	sleepSeconds, err := parseNonNegativeInt("loop.sleep_seconds", l.editData.SleepSeconds)
	if err != nil {
		return err
	}
	maxIterations, err := parseNonNegativeInt("loop.max_iterations", l.editData.MaxIterations)
	if err != nil {
		return err
	}
	maxStalled, err := parseNonNegativeInt("loop.max_stalled", l.editData.MaxStalled)
	if err != nil {
		return err
	}
	maxRepair, err := parseNonNegativeInt("loop.max_repair_attempts", l.editData.MaxRepairAttempts)
	if err != nil {
		return err
	}
	if _, err := parseOnlyTags(l.editData.OnlyTags); err != nil {
		return err
	}
	l.overrides.SleepSeconds = sleepSeconds
	l.overrides.MaxIterations = maxIterations
	l.overrides.MaxStalled = maxStalled
	l.overrides.MaxRepairAttempts = maxRepair
	l.overrides.OnlyTags = strings.TrimSpace(l.editData.OnlyTags)
	l.overrides.RequireMain = l.editData.RequireMain
	l.overrides.AutoCommit = l.editData.AutoCommit
	l.overrides.AutoPush = l.editData.AutoPush
	l.overrides.Runner = strings.TrimSpace(l.editData.Runner)
	l.overrides.RunnerArgs = parseArgsLines(l.editData.RunnerArgs)
	reasoningEffort := strings.ToLower(strings.TrimSpace(l.editData.ReasoningEffort))
	if reasoningEffort == "" {
		reasoningEffort = "auto"
	}
	l.overrides.ReasoningEffort = reasoningEffort
	l.overrides.ForceContextBuilder = l.editData.ForceContextBuilder
	return nil
}

type loopFormData struct {
	SleepSeconds        string
	MaxIterations       string
	MaxStalled          string
	MaxRepairAttempts   string
	OnlyTags            string
	RequireMain         bool
	AutoCommit          bool
	AutoPush            bool
	Runner              string
	RunnerArgs          string
	ReasoningEffort     string
	ForceContextBuilder bool
}

func loopFormDataFromOverrides(overrides loopOverrides) loopFormData {
	return loopFormData{
		SleepSeconds:        fmt.Sprintf("%d", overrides.SleepSeconds),
		MaxIterations:       fmt.Sprintf("%d", overrides.MaxIterations),
		MaxStalled:          fmt.Sprintf("%d", overrides.MaxStalled),
		MaxRepairAttempts:   fmt.Sprintf("%d", overrides.MaxRepairAttempts),
		OnlyTags:            overrides.OnlyTags,
		RequireMain:         overrides.RequireMain,
		AutoCommit:          overrides.AutoCommit,
		AutoPush:            overrides.AutoPush,
		Runner:              overrides.Runner,
		RunnerArgs:          formatArgsLines(overrides.RunnerArgs),
		ReasoningEffort:     runnerargs.DisplayEffort(overrides.ReasoningEffort),
		ForceContextBuilder: overrides.ForceContextBuilder,
	}
}

func parseOnlyTags(value string) ([]string, error) {
	parsed := pin.ParseTagList(value)
	if len(parsed.Unknown) > 0 {
		return nil, fmt.Errorf(
			"loop.only_tags has unsupported tag(s): %s (supported: %s)",
			strings.Join(parsed.Unknown, ", "),
			strings.Join(pin.SupportedTags(), ", "),
		)
	}
	return parsed.Tags, nil
}

func loopModeLabel(runOnce bool) string {
	if runOnce {
		return "once"
	}
	return "continuous"
}

func (l *loopView) Resize(width int, height int) {
	l.width = width
	l.height = height
	controlsLines := strings.Count(l.controlsView(), "\n") + 1
	extraLines := 4
	stateView := l.stateView()
	stateLines := strings.Count(stateView, "\n")
	if stateView != "" {
		stateLines++
	}
	logHeight := height - (controlsLines + extraLines + stateLines)
	if logHeight < 0 {
		logHeight = 0
	}
	resizeViewportToFit(&l.viewport, max(0, width), max(0, logHeight), paddedViewportStyle)

	if l.editForm != nil {
		formHeight := height - 3
		if formHeight < 1 {
			formHeight = 1
		}
		l.editForm = l.editForm.WithWidth(max(1, width))
		l.editForm = l.editForm.WithHeight(max(1, formHeight))
	}
}

func (l *loopView) SetConfig(cfg config.Config, locations paths.Locations) {
	l.cfg = cfg
	l.locations = locations
	if l.mode != loopIdle {
		return
	}
	l.overrides = loopOverrides{
		SleepSeconds:        cfg.Loop.SleepSeconds,
		MaxIterations:       cfg.Loop.MaxIterations,
		MaxStalled:          cfg.Loop.MaxStalled,
		MaxRepairAttempts:   cfg.Loop.MaxRepairAttempts,
		OnlyTags:            cfg.Loop.OnlyTags,
		RequireMain:         cfg.Loop.RequireMain,
		AutoCommit:          cfg.Git.AutoCommit,
		AutoPush:            cfg.Git.AutoPush,
		Runner:              cfg.Loop.Runner,
		RunnerArgs:          cfg.Loop.RunnerArgs,
		ReasoningEffort:     cfg.Loop.ReasoningEffort,
		ForceContextBuilder: l.overrides.ForceContextBuilder,
		DirtyRepoStart:      cfg.Loop.DirtyRepo.StartPolicy,
		DirtyRepoDuring:     cfg.Loop.DirtyRepo.DuringPolicy,
		AllowUntracked:      cfg.Loop.DirtyRepo.AllowUntracked,
		QuarantineClean:     cfg.Loop.DirtyRepo.QuarantineCleanUntracked,
	}
}

func (l *loopView) Focus() {}

func (l *loopView) Blur() {}

func (l *loopView) appendLogLine(line string) {
	l.appendLogLines([]string{line})
}

func (l *loopView) appendLogLines(lines []string) {
	if len(lines) == 0 {
		return
	}
	l.persistLoopLines(lines)
	atBottom := l.viewport.AtBottom()
	l.logBuf.AppendLines(lines)
	l.pendingViewportLines += len(lines)
	if l.mode == loopRunning {
		threshold := loopLogFlushThreshold(atBottom)
		if !shouldFlushLogViewport(l.pendingViewportLines, threshold, l.lastViewportFlush) {
			return
		}
	}
	l.flushLogViewport(atBottom)
}

func (l *loopView) LogLines() []string {
	return l.logBuf.Lines()
}

func (l *loopView) flushLogViewport(wasAtBottom bool) {
	version := l.logBuf.Version()
	if version == l.lastViewportVersion {
		return
	}
	l.viewport.SetContent(l.logBuf.ContentString())
	l.lastViewportVersion = version
	l.pendingViewportLines = 0
	l.lastViewportFlush = time.Now()
	if wasAtBottom {
		l.viewport.GotoBottom()
	}
}

func loopLogFlushThreshold(atBottom bool) int {
	if atBottom {
		return 32
	}
	return 256
}

func listenLoopLogs(logCh <-chan string, runID int) tea.Cmd {
	return func() tea.Msg {
		return loopLogBatchMsg{batch: drainLogChannel(runID, logCh, 64)}
	}
}

type loopStateSink struct {
	ch chan loop.State
}

func (s loopStateSink) Update(state loop.State) {
	if s.ch == nil {
		return
	}
	select {
	case s.ch <- state:
		return
	default:
	}
	select {
	case <-s.ch:
	default:
	}
	select {
	case s.ch <- state:
	default:
	}
}

func listenLoopState(stateCh <-chan loop.State, runID int) tea.Cmd {
	return func() tea.Msg {
		state, ok := <-stateCh
		if !ok {
			return loopStateMsg{runID: runID, done: true}
		}
		return loopStateMsg{runID: runID, state: state}
	}
}

func (l *loopView) loopOutputPath() string {
	if strings.TrimSpace(l.cfg.Paths.CacheDir) == "" {
		return ""
	}
	return filepath.Join(l.cfg.Paths.CacheDir, "loop_output.log")
}

func (l *loopView) startPersistingOutput() {
	path := l.loopOutputPath()
	if path == "" {
		return
	}
	if l.output == nil {
		l.output = &outputFileWriter{}
	}
	if err := l.output.Reset(path); err != nil {
		l.outputErr = err.Error()
		l.logOutputError(err, path)
		return
	}
	l.outputErr = ""
}

func (l *loopView) stopPersistingOutput() {
	if l.output == nil {
		return
	}
	if err := l.output.Close(); err != nil {
		if l.outputErr == "" {
			l.outputErr = err.Error()
		}
		l.logOutputError(err, l.loopOutputPath())
	}
}

func (l *loopView) persistLoopLines(lines []string) {
	if l.output == nil {
		return
	}
	if err := l.output.AppendLines(lines); err != nil {
		l.outputErr = err.Error()
		l.logOutputError(err, l.loopOutputPath())
		_ = l.output.Close()
		l.output = nil
	}
}

func (l *loopView) logOutputError(err error, path string) {
	if l.logger == nil || err == nil {
		return
	}
	l.logger.Error("loop.output.persist.error", map[string]any{
		"error": err.Error(),
		"path":  path,
	})
}
