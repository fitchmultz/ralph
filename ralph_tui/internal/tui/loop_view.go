// Package tui provides the Run Loop screen.
// Entrypoint: loopView.
package tui

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
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
	logCh                chan string
	logRunID             int
	logger               *tuiLogger
	output               *outputFileWriter
	width                int
	height               int
	pendingViewportLines int
	lastViewportVersion  uint64
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
}

type loopMode int

const (
	loopIdle loopMode = iota
	loopRunning
	loopStopping
	loopEditing
)

type loopResultMsg struct {
	err error
}

type loopLogBatchMsg struct {
	batch logBatch
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
		},
		mode:   loopIdle,
		status: "Idle",
	}
}

func (l *loopView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
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
			return l.start(true)
		case key.Matches(msg, keys.RunLoopContinuous) && l.mode == loopIdle:
			return l.start(false)
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
	return l.start(true)
}

func (l *loopView) HandlesTabNavigation() bool {
	return l.mode == loopEditing && l.editForm != nil
}

func (l *loopView) View() string {
	head := "Run Loop"
	status := l.statusLine()
	controls := l.controlsView()
	if l.mode == loopEditing && l.editForm != nil {
		return withFinalNewline(head + "\n" + status + "\n\n" + l.editForm.View())
	}
	logs := l.viewport.View()
	return withFinalNewline(head + "\n" + status + "\n\n" + controls + "\n\n" + logs)
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
	effortResult := runnerargs.ApplyReasoningEffort(l.overrides.Runner, l.overrides.RunnerArgs, l.overrides.ReasoningEffort)
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
		fmt.Sprintf("Runner: %s", l.overrides.Runner),
		fmt.Sprintf("Runner args: %d", len(l.overrides.RunnerArgs)),
		fmt.Sprintf("Reasoning effort: %s (effective: %s)", runnerargs.DisplayEffort(l.overrides.ReasoningEffort), effectiveLabel),
		fmt.Sprintf("Force context_builder: %s (mandatory: %s)", yesNo(l.overrides.ForceContextBuilder), yesNo(mandatory)),
		"Keys: r run once | c continuous | s stop | e edit overrides | p force ctx builder",
	}
	return strings.Join(lines, "\n")
}

func (l *loopView) start(runOnce bool) tea.Cmd {
	l.err = ""
	l.outputErr = ""
	l.status = "Running"
	l.mode = loopRunning
	l.startPersistingOutput()

	ctx, cancel := context.WithCancel(context.Background())
	l.cancel = cancel

	logCh := newLogChannel()
	l.logCh = logCh
	l.logRunID++
	runID := l.logRunID
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
			"runner":                l.overrides.Runner,
			"runner_args_count":     len(applied.Args),
			"reasoning_effort":      runnerargs.DisplayEffort(l.overrides.ReasoningEffort),
			"force_context_builder": l.overrides.ForceContextBuilder,
		})
	}

	runCmd := func() tea.Msg {
		logger := loopLogger{
			write: func(line string) {
				sendLineBlocking(logCh, line)
			},
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
			OnlyTags:            splitTags(l.overrides.OnlyTags),
			Once:                runOnce,
			RequireMain:         l.overrides.RequireMain,
			AutoCommit:          l.overrides.AutoCommit,
			AutoPush:            l.overrides.AutoPush,
			ForceContextBuilder: l.overrides.ForceContextBuilder,
			RedactionMode:       l.cfg.Logging.RedactionMode,
			Logger:              logger,
		})
		if err != nil {
			close(logCh)
			return loopResultMsg{err: err}
		}
		if err := runner.Run(ctx); err != nil {
			close(logCh)
			return loopResultMsg{err: err}
		}
		close(logCh)
		return loopResultMsg{}
	}

	return tea.Batch(runCmd, listenLoopLogs(logCh, runID))
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

func splitTags(value string) []string {
	if strings.TrimSpace(value) == "" {
		return []string{}
	}
	parts := strings.Split(value, ",")
	result := make([]string, 0, len(parts))
	for _, part := range parts {
		trimmed := strings.TrimSpace(part)
		if trimmed != "" {
			result = append(result, trimmed)
		}
	}
	return result
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
	logHeight := height - (controlsLines + 4)
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
		if l.pendingViewportLines < threshold {
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
