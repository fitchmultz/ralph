// Package tui provides the Run Loop screen.
// Entrypoint: loopView.
package tui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/charmbracelet/lipgloss"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/config"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/loop"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/paths"
)

type loopView struct {
	cfg       config.Config
	locations paths.Locations
	viewport  viewport.Model
	overrides loopOverrides
	mode      loopMode
	status    string
	err       string
	logs      []string
	cancel    context.CancelFunc
	editForm  *huh.Form
	editData  loopFormData
	logCh     chan string
}

type loopOverrides struct {
	SleepSeconds      int
	MaxIterations     int
	MaxStalled        int
	MaxRepairAttempts int
	OnlyTags          string
	RequireMain       bool
	AutoCommit        bool
	AutoPush          bool
}

type loopMode int

const (
	loopIdle loopMode = iota
	loopRunning
	loopEditing
)

type loopTickMsg struct{}

type loopResultMsg struct {
	err error
}

type loopLogMsg struct {
	line string
}

type loopLogDoneMsg struct{}

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
		overrides: loopOverrides{
			SleepSeconds:      cfg.Loop.SleepSeconds,
			MaxIterations:     cfg.Loop.MaxIterations,
			MaxStalled:        cfg.Loop.MaxStalled,
			MaxRepairAttempts: cfg.Loop.MaxRepairAttempts,
			OnlyTags:          cfg.Loop.OnlyTags,
			RequireMain:       cfg.Loop.RequireMain,
			AutoCommit:        cfg.Git.AutoCommit,
			AutoPush:          cfg.Git.AutoPush,
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
		} else {
			l.err = ""
			l.status = "Stopped"
		}
		l.cancel = nil
		return nil
	case loopLogMsg:
		l.appendLogLine(msg.line)
		if l.logCh != nil {
			return listenLoopLogs(l.logCh)
		}
		return nil
	case loopLogDoneMsg:
		l.logCh = nil
		return nil
	case loopTickMsg:
		if l.mode == loopRunning {
			return l.tickCmd()
		}
		return nil
	case tea.KeyMsg:
		switch {
		case key.Matches(msg, keys.RunLoopOnce) && l.mode == loopIdle:
			return l.start(true)
		case key.Matches(msg, keys.RunLoopContinuous) && l.mode == loopIdle:
			return l.start(false)
		case key.Matches(msg, keys.StopLoop):
			l.stop()
			l.mode = loopIdle
			l.status = "Stopped"
			l.err = ""
			return nil
		case key.Matches(msg, keys.EditLoopConfig) && l.mode == loopIdle:
			l.beginEdit()
			return nil
		}
	}

	if l.mode == loopIdle {
		updated, cmd := l.viewport.Update(msg)
		l.viewport = updated
		return cmd
	}
	return nil
}

func (l *loopView) View() string {
	head := "Run Loop"
	status := l.statusLine()
	controls := l.controlsView()
	if l.mode == loopEditing && l.editForm != nil {
		return strings.TrimSpace(head+"\n"+status+"\n\n"+l.editForm.View()) + "\n"
	}
	logs := l.viewport.View()
	return strings.TrimSpace(head+"\n"+status+"\n\n"+controls+"\n\n"+logs) + "\n"
}

func (l *loopView) statusLine() string {
	if l.err != "" {
		return fmt.Sprintf("Error: %s", l.err)
	}
	return l.status
}

func (l *loopView) controlsView() string {
	lines := []string{
		fmt.Sprintf("Sleep seconds: %d", l.overrides.SleepSeconds),
		fmt.Sprintf("Max iterations: %d", l.overrides.MaxIterations),
		fmt.Sprintf("Max stalled: %d", l.overrides.MaxStalled),
		fmt.Sprintf("Max repair attempts: %d", l.overrides.MaxRepairAttempts),
		fmt.Sprintf("Only tags: %s", l.overrides.OnlyTags),
		fmt.Sprintf("Require main: %s", yesNo(l.overrides.RequireMain)),
		fmt.Sprintf("Auto commit: %s", yesNo(l.overrides.AutoCommit)),
		fmt.Sprintf("Auto push: %s", yesNo(l.overrides.AutoPush)),
		"Keys: r run once | c continuous | s stop | e edit overrides",
	}
	return strings.Join(lines, "\n")
}

func (l *loopView) tickCmd() tea.Cmd {
	return func() tea.Msg {
		time.Sleep(500 * time.Millisecond)
		return loopTickMsg{}
	}
}

func (l *loopView) start(runOnce bool) tea.Cmd {
	l.err = ""
	l.status = "Running"
	l.mode = loopRunning

	ctx, cancel := context.WithCancel(context.Background())
	l.cancel = cancel

	logCh := make(chan string, 1024)
	l.logCh = logCh

	runCmd := func() tea.Msg {
		logger := loopLogger{
			write: func(line string) {
				select {
				case logCh <- line:
				default:
				}
			},
		}
		runner, err := loop.NewRunner(loop.Options{
			RepoRoot:          l.locations.RepoRoot,
			PinDir:            l.cfg.Paths.PinDir,
			PromptPath:        "",
			SupervisorPrompt:  "",
			Runner:            "codex",
			RunnerArgs:        []string{},
			SleepSeconds:      l.overrides.SleepSeconds,
			MaxIterations:     l.overrides.MaxIterations,
			MaxStalled:        l.overrides.MaxStalled,
			MaxRepairAttempts: l.overrides.MaxRepairAttempts,
			OnlyTags:          splitTags(l.overrides.OnlyTags),
			Once:              runOnce,
			RequireMain:       l.overrides.RequireMain,
			AutoCommit:        l.overrides.AutoCommit,
			AutoPush:          l.overrides.AutoPush,
			Logger:            logger,
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

	return tea.Batch(runCmd, listenLoopLogs(logCh), l.tickCmd())
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
	return nil
}

type loopFormData struct {
	SleepSeconds      string
	MaxIterations     string
	MaxStalled        string
	MaxRepairAttempts string
	OnlyTags          string
	RequireMain       bool
	AutoCommit        bool
	AutoPush          bool
}

func loopFormDataFromOverrides(overrides loopOverrides) loopFormData {
	return loopFormData{
		SleepSeconds:      fmt.Sprintf("%d", overrides.SleepSeconds),
		MaxIterations:     fmt.Sprintf("%d", overrides.MaxIterations),
		MaxStalled:        fmt.Sprintf("%d", overrides.MaxStalled),
		MaxRepairAttempts: fmt.Sprintf("%d", overrides.MaxRepairAttempts),
		OnlyTags:          overrides.OnlyTags,
		RequireMain:       overrides.RequireMain,
		AutoCommit:        overrides.AutoCommit,
		AutoPush:          overrides.AutoPush,
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

func (l *loopView) Resize(width int, height int) {
	l.viewport.Width = max(10, width)
	l.viewport.Height = max(5, height)
	l.viewport.Style = lipgloss.NewStyle().Padding(0, 1)
}

func (l *loopView) SetConfig(cfg config.Config, locations paths.Locations) {
	l.cfg = cfg
	l.locations = locations
	if l.mode != loopIdle {
		return
	}
	l.overrides = loopOverrides{
		SleepSeconds:      cfg.Loop.SleepSeconds,
		MaxIterations:     cfg.Loop.MaxIterations,
		MaxStalled:        cfg.Loop.MaxStalled,
		MaxRepairAttempts: cfg.Loop.MaxRepairAttempts,
		OnlyTags:          cfg.Loop.OnlyTags,
		RequireMain:       cfg.Loop.RequireMain,
		AutoCommit:        cfg.Git.AutoCommit,
		AutoPush:          cfg.Git.AutoPush,
	}
}

func (l *loopView) Focus() {}

func (l *loopView) Blur() {}

func (l *loopView) appendLogLine(line string) {
	l.logs = append(l.logs, line)
	if len(l.logs) > 2000 {
		l.logs = l.logs[len(l.logs)-2000:]
	}
	atBottom := l.viewport.AtBottom()
	l.viewport.SetContent(strings.Join(l.logs, "\n"))
	if atBottom {
		l.viewport.GotoBottom()
	}
}

func listenLoopLogs(logCh <-chan string) tea.Cmd {
	return func() tea.Msg {
		line, ok := <-logCh
		if !ok {
			return loopLogDoneMsg{}
		}
		return loopLogMsg{line: line}
	}
}
