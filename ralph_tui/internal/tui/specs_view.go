// Package tui provides the Build Specs screen for the Ralph TUI.
// Entrypoint: specsView.
package tui

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/glamour"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
)

type specsView struct {
	cfg               config.Config
	locations         paths.Locations
	runner            specs.Runner
	runnerArgs        []string
	reasoningEffort   string
	interactive       bool
	innovate          bool
	innovateExplicit  bool
	autofillScout     bool
	autofillExplicit  bool
	effectiveInnovate bool
	autoEnabled       bool
	preview           string
	previewErr        string
	previewLoading    bool
	previewDirty      bool
	status            string
	err               string
	refreshErr        string
	persistErr        string
	previewViewport   viewport.Model
	logViewport       viewport.Model
	previewWidth      int
	running           bool
	diffStat          string
	runLogs           []string
	lastRunOutput     string
	buildCancel       context.CancelFunc
	logCh             chan string
	logRunID          int
	pendingResult     *specsBuildResultMsg
	queueStamp        fileStamp
	promptStamp       fileStamp
	logger            *tuiLogger
	output            *outputFileWriter
}

type specsBuildResultMsg struct {
	err       error
	pinErr    error
	diffStat  string
	effective bool
}

type specsLogBatchMsg struct {
	batch logBatch
}

type specsPreviewMsg struct {
	preview   string
	err       error
	effective bool
	auto      bool
}

func newSpecsView(cfg config.Config, locations paths.Locations) (*specsView, error) {
	vp := viewport.New(80, 20)
	logViewport := viewport.New(80, 20)
	vp.Style = paddedViewportStyle
	logViewport.Style = paddedViewportStyle
	view := &specsView{
		cfg:             cfg,
		locations:       locations,
		runner:          specs.Runner(cfg.Specs.Runner),
		runnerArgs:      cfg.Specs.RunnerArgs,
		reasoningEffort: cfg.Specs.ReasoningEffort,
		autofillScout:   cfg.Specs.AutofillScout,
		previewViewport: vp,
		logViewport:     logViewport,
		previewWidth:    80,
		previewDirty:    true,
	}
	if stamp, err := getFileStamp(filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")); err == nil {
		view.queueStamp = stamp
	}
	if stamp, err := getFileStamp(filepath.Join(cfg.Paths.PinDir, "specs_builder.md")); err == nil {
		view.promptStamp = stamp
	}
	return view, nil
}

func (s *specsView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
	switch msg := msg.(type) {
	case specsBuildResultMsg:
		s.pendingResult = &msg
		if s.logCh == nil {
			cmd := s.applyBuildResult(msg)
			s.pendingResult = nil
			return cmd
		}
		return nil
	case specsLogBatchMsg:
		if msg.batch.RunID != s.logRunID {
			return nil
		}
		if len(msg.batch.Lines) > 0 {
			s.appendRunLogs(msg.batch.Lines)
		}
		if msg.batch.Done {
			s.logCh = nil
			s.stopPersistingOutput()
			s.finalizeRunOutput()
			if s.pendingResult != nil {
				cmd := s.applyBuildResult(*s.pendingResult)
				s.pendingResult = nil
				s.running = false
				return cmd
			}
			s.running = false
			return nil
		}
		if s.logCh != nil {
			return listenSpecsLogs(s.logCh, s.logRunID)
		}
		return nil
	case specsPreviewMsg:
		s.previewLoading = false
		if msg.err != nil {
			s.previewErr = msg.err.Error()
			s.setRefreshError("render preview", msg.err)
			s.preview = ""
			if s.previewDirty && !s.running {
				return s.refreshPreviewAsync()
			}
			return nil
		}
		s.previewErr = ""
		s.clearRefreshError()
		s.preview = msg.preview
		s.effectiveInnovate = msg.effective
		s.autoEnabled = msg.auto
		s.previewViewport.SetContent(msg.preview)
		s.previewViewport.GotoTop()
		if s.previewDirty && !s.running {
			return s.refreshPreviewAsync()
		}
		return nil
	case tea.KeyMsg:
		switch {
		case key.Matches(msg, keys.ToggleInteractive) && !s.running:
			s.interactive = !s.interactive
			return s.requestPreviewRefresh()
		case key.Matches(msg, keys.ToggleInnovate) && !s.running:
			s.innovate = !s.innovate
			s.innovateExplicit = true
			return s.requestPreviewRefresh()
		case key.Matches(msg, keys.ToggleAutofill) && !s.running:
			s.autofillScout = !s.autofillScout
			s.autofillExplicit = true
			return s.requestPreviewRefresh()
		case key.Matches(msg, keys.RunSpecs):
			if s.running {
				return nil
			}
			s.running = true
			s.status = "Running specs build..."
			s.err = ""
			s.persistErr = ""
			return s.runBuildCmd()
		case key.Matches(msg, keys.StopSpecs):
			if !s.running {
				return nil
			}
			s.cancelBuild()
			return nil
		case msg.String() == "j":
			s.activeViewport().LineDown(1)
			return nil
		case msg.String() == "k":
			s.activeViewport().LineUp(1)
			return nil
		}
	}

	if s.running {
		updated, cmd := s.logViewport.Update(msg)
		s.logViewport = updated
		return cmd
	}
	updated, cmd := s.previewViewport.Update(msg)
	s.previewViewport = updated
	return cmd
}

func (s *specsView) View() string {
	header := "Build Specs"
	status := s.statusLine()
	options := s.optionsView()
	preview := s.previewView()
	return withFinalNewline(header + "\n" + status + "\n\n" + options + "\n\n" + preview)
}

func (s *specsView) statusLine() string {
	if s.err != "" {
		return fmt.Sprintf("Error: %s", s.err)
	}
	if s.refreshErr != "" {
		return fmt.Sprintf("Error: %s", s.refreshErr)
	}
	if s.persistErr != "" {
		return fmt.Sprintf("Persist error: %s", s.persistErr)
	}
	if s.status != "" {
		return s.status
	}
	if s.previewLoading && !s.running {
		return "Rendering preview..."
	}
	return ""
}

func (s *specsView) optionsView() string {
	innovate := "off"
	if s.effectiveInnovate {
		innovate = "on"
		if s.autoEnabled {
			innovate += " (auto)"
		}
	}
	lines := []string{
		fmt.Sprintf("Runner: %s", s.runner),
		fmt.Sprintf("Runner args: %d", len(s.runnerArgs)),
		fmt.Sprintf("Reasoning effort: %s", displayReasoningEffort(s.reasoningEffort)),
		fmt.Sprintf("Interactive: %s", yesNo(s.interactive)),
		fmt.Sprintf("Innovate: %s", innovate),
		fmt.Sprintf("Autofill scout: %s", yesNo(s.autofillScout)),
		"Keys: e settings (runner/args/effort) | i interactive | n innovate | a autofill | r run build | s stop build",
		"Scroll: \u2191/\u2193 PgUp/PgDn (Tab to focus)",
	}
	return strings.Join(lines, "\n")
}

func (s *specsView) previewView() string {
	if s.running {
		return s.logViewport.View()
	}
	if s.previewErr != "" {
		return fmt.Sprintf("Prompt preview error: %s", s.previewErr)
	}
	if s.preview == "" {
		return "Prompt preview unavailable."
	}
	return s.previewViewport.View()
}

func (s *specsView) refreshPreviewAsync() tea.Cmd {
	s.previewLoading = true
	s.previewDirty = false
	s.previewErr = ""

	cfg := s.cfg
	interactive := s.interactive
	innovate := s.innovate
	innovateExplicit := s.innovateExplicit
	autofillScout := s.autofillScout
	lastRunOutput := s.lastRunOutput
	diffStat := s.diffStat
	previewWidth := s.previewWidth
	return func() tea.Msg {
		queuePath := filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")
		effective, err := specs.ResolveInnovate(queuePath, innovate, innovateExplicit, autofillScout)
		if err != nil {
			return specsPreviewMsg{err: err}
		}
		autoEnabled := !innovateExplicit && autofillScout && !innovate && effective

		promptPath := filepath.Join(cfg.Paths.PinDir, "specs_builder.md")
		prompt, err := specs.FillPrompt(promptPath, interactive, effective)
		if err != nil {
			return specsPreviewMsg{err: err}
		}
		renderer, err := buildRenderer(previewWidth)
		if err != nil {
			return specsPreviewMsg{err: err}
		}
		rendered, err := renderer.Render(prompt)
		if err != nil {
			return specsPreviewMsg{err: err}
		}
		if lastRunOutput != "" {
			rendered = rendered + "\n\nBuild output:\n" + lastRunOutput
		}
		if diffStat != "" {
			rendered = rendered + "\n\nDiff stat:\n" + diffStat
		}
		return specsPreviewMsg{preview: rendered, effective: effective, auto: autoEnabled}
	}
}

func (s *specsView) requestPreviewRefresh() tea.Cmd {
	if s.previewLoading {
		s.previewDirty = true
		return nil
	}
	return s.refreshPreviewAsync()
}

func (s *specsView) runBuildCmd() tea.Cmd {
	s.resetRunLogs()
	s.startPersistingOutput()
	logCh := make(chan string, 1024)
	s.logCh = logCh
	s.logRunID++
	runID := s.logRunID
	s.pendingResult = nil
	s.logViewport.SetContent("")
	s.logViewport.GotoTop()
	ctx, cancel := context.WithCancel(context.Background())
	s.buildCancel = cancel
	if s.logger != nil {
		appliedArgs := applyReasoningEffort(string(s.runner), s.runnerArgs, s.reasoningEffort, "medium")
		s.logger.Info("specs.run.start", map[string]any{
			"runner":            s.runner,
			"runner_args_count": len(appliedArgs),
			"reasoning_effort":  displayReasoningEffort(s.reasoningEffort),
			"interactive":       s.interactive,
			"innovate":          s.innovate,
			"autofillScout":     s.autofillScout,
		})
	}

	sink := logChannelSink{ch: logCh}
	writer := newStreamWriter(sink)

	runCmd := func() tea.Msg {
		defer cancel()
		defer close(logCh)
		result, err := specs.Build(ctx, specs.BuildOptions{
			RepoRoot:         s.locations.RepoRoot,
			PinDir:           s.cfg.Paths.PinDir,
			Runner:           s.runner,
			RunnerArgs:       applyReasoningEffort(string(s.runner), s.runnerArgs, s.reasoningEffort, "medium"),
			Interactive:      s.interactive,
			Innovate:         s.innovate,
			InnovateExplicit: s.innovateExplicit,
			AutofillScout:    s.autofillScout,
			Stdout:           writer,
			Stderr:           writer,
		})
		writer.Flush()
		if err != nil {
			return specsBuildResultMsg{err: err}
		}
		files := pin.ResolveFiles(s.cfg.Paths.PinDir)
		pinErr := pin.ValidatePin(files)
		diffStat, diffErr := specs.GitDiffStat(s.locations.RepoRoot)
		if diffErr != nil {
			diffStat = ""
		}
		return specsBuildResultMsg{pinErr: pinErr, diffStat: diffStat, effective: result.EffectiveInnovate}
	}

	return tea.Batch(runCmd, listenSpecsLogs(logCh, runID))
}

func yesNo(value bool) string {
	if value {
		return "on"
	}
	return "off"
}

func (s *specsView) Resize(width int, height int) {
	optionsLines := strings.Count(s.optionsView(), "\n") + 1
	reserved := 1 + 1 + 1 + optionsLines + 1
	previewHeight := height - reserved
	if previewHeight < 0 {
		previewHeight = 0
	}
	resizeViewportToFit(&s.previewViewport, max(0, width), max(0, previewHeight), paddedViewportStyle)
	resizeViewportToFit(&s.logViewport, max(0, width), max(0, previewHeight), paddedViewportStyle)
	s.previewWidth = max(1, s.previewViewport.Width)
	s.previewDirty = true
}

func buildRenderer(previewWidth int) (*glamour.TermRenderer, error) {
	wrapWidth := previewWidth
	if wrapWidth <= 0 {
		wrapWidth = 80
	}
	return glamour.NewTermRenderer(
		glamour.WithAutoStyle(),
		glamour.WithWordWrap(wrapWidth),
	)
}

func (s *specsView) SetConfig(cfg config.Config, locations paths.Locations) {
	s.cfg = cfg
	s.locations = locations
	if !s.autofillExplicit {
		s.autofillScout = cfg.Specs.AutofillScout
	}
	if !s.running {
		s.runner = specs.Runner(cfg.Specs.Runner)
		s.runnerArgs = cfg.Specs.RunnerArgs
		s.reasoningEffort = cfg.Specs.ReasoningEffort
	}
	if stamp, err := getFileStamp(filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")); err == nil {
		s.queueStamp = stamp
	}
	if stamp, err := getFileStamp(filepath.Join(cfg.Paths.PinDir, "specs_builder.md")); err == nil {
		s.promptStamp = stamp
	}
	if !s.running {
		s.previewDirty = true
	}
}

func (s *specsView) RefreshIfNeeded() tea.Cmd {
	if s.running {
		return nil
	}
	if s.previewDirty {
		return s.requestPreviewRefresh()
	}
	queuePath := filepath.Join(s.cfg.Paths.PinDir, "implementation_queue.md")
	queueStamp, queueChanged, queueErr := fileChanged(queuePath, s.queueStamp)
	if queueErr != nil {
		s.setRefreshError("watch implementation_queue.md", queueErr)
		return nil
	}
	promptPath := filepath.Join(s.cfg.Paths.PinDir, "specs_builder.md")
	promptStamp, promptChanged, promptErr := fileChanged(promptPath, s.promptStamp)
	if promptErr != nil {
		s.setRefreshError("watch specs_builder.md", promptErr)
		return nil
	}
	if s.previewErr == "" && s.refreshErr != "" {
		s.clearRefreshError()
	}
	if queueChanged {
		s.queueStamp = queueStamp
	}
	if promptChanged {
		s.promptStamp = promptStamp
	}
	if queueChanged || promptChanged {
		return s.requestPreviewRefresh()
	}
	return nil
}

func (s *specsView) RefreshPreviewCmd() tea.Cmd {
	if s.previewDirty {
		return s.requestPreviewRefresh()
	}
	return nil
}

func (s *specsView) Focus() {}

func (s *specsView) Blur() {}

func (s *specsView) activeViewport() *viewport.Model {
	if s.running {
		return &s.logViewport
	}
	return &s.previewViewport
}

func (s *specsView) resetRunLogs() {
	s.runLogs = nil
	s.lastRunOutput = ""
	s.diffStat = ""
}

func (s *specsView) appendRunLog(line string) {
	s.appendRunLogs([]string{line})
}

func (s *specsView) appendRunLogs(lines []string) {
	if len(lines) == 0 {
		return
	}
	s.persistSpecsLines(lines)
	const maxLines = 500
	s.runLogs = append(s.runLogs, lines...)
	if len(s.runLogs) > maxLines {
		s.runLogs = s.runLogs[len(s.runLogs)-maxLines:]
	}
	atBottom := s.logViewport.AtBottom()
	s.logViewport.SetContent(strings.Join(s.runLogs, "\n"))
	if atBottom {
		s.logViewport.GotoBottom()
	}
}

func (s *specsView) finalizeRunOutput() {
	if len(s.runLogs) == 0 {
		s.lastRunOutput = ""
		return
	}
	s.lastRunOutput = strings.Join(s.runLogs, "\n")
}

func (s *specsView) applyBuildResult(msg specsBuildResultMsg) tea.Cmd {
	s.buildCancel = nil
	if msg.err != nil {
		if errors.Is(msg.err, context.Canceled) {
			s.err = ""
			s.status = "Specs build canceled."
			s.previewDirty = true
			if s.logger != nil {
				s.logger.Info("specs.run.canceled", map[string]any{})
			}
			return s.RefreshPreviewCmd()
		}
		s.err = msg.err.Error()
		s.status = ""
		s.previewDirty = true
		if s.logger != nil {
			s.logger.Error("specs.run.error", map[string]any{"error": msg.err.Error()})
		}
		return s.RefreshPreviewCmd()
	}
	if msg.pinErr != nil {
		s.err = msg.pinErr.Error()
		s.status = ""
		s.previewDirty = true
		if s.logger != nil {
			s.logger.Error("specs.pin.error", map[string]any{"error": msg.pinErr.Error()})
		}
		return s.RefreshPreviewCmd()
	}
	s.err = ""
	s.status = ">> [RALPH] Pin validation OK."
	if summary := summarizeDiffStat(msg.diffStat); summary != "" {
		s.status = s.status + " | Diff: " + summary
	}
	s.diffStat = msg.diffStat
	s.previewDirty = true
	if s.logger != nil {
		s.logger.Info("specs.run.complete", map[string]any{
			"effective_innovate": msg.effective,
			"diff_stat":          summarizeDiffStat(msg.diffStat),
		})
	}
	return s.RefreshPreviewCmd()
}

func (s *specsView) specsOutputPath() string {
	if strings.TrimSpace(s.cfg.Paths.CacheDir) == "" {
		return ""
	}
	return filepath.Join(s.cfg.Paths.CacheDir, "specs_output.log")
}

func (s *specsView) startPersistingOutput() {
	path := s.specsOutputPath()
	if path == "" {
		return
	}
	if s.output == nil {
		s.output = &outputFileWriter{}
	}
	if err := s.output.Reset(path); err != nil {
		s.persistErr = err.Error()
		s.logOutputError("specs.output.persist.error", err, path)
		return
	}
	s.persistErr = ""
}

func (s *specsView) stopPersistingOutput() {
	if s.output == nil {
		return
	}
	if err := s.output.Close(); err != nil {
		if s.persistErr == "" {
			s.persistErr = err.Error()
		}
		s.logOutputError("specs.output.persist.error", err, s.specsOutputPath())
	}
}

func (s *specsView) cancelBuild() {
	if s.buildCancel == nil {
		return
	}
	s.status = "Canceling specs build..."
	s.buildCancel()
}

func (s *specsView) persistSpecsLines(lines []string) {
	if s.output == nil {
		return
	}
	if err := s.output.AppendLines(lines); err != nil {
		s.persistErr = err.Error()
		s.logOutputError("specs.output.persist.error", err, s.specsOutputPath())
		_ = s.output.Close()
		s.output = nil
	}
}

func (s *specsView) setRefreshError(context string, err error) {
	if err == nil {
		return
	}
	message := context + ": " + err.Error()
	s.refreshErr = message
	s.logOutputError("specs.refresh.error", err, context)
}

func (s *specsView) clearRefreshError() {
	s.refreshErr = ""
}

func (s *specsView) logOutputError(event string, err error, detail string) {
	if s.logger == nil || err == nil {
		return
	}
	s.logger.Error(event, map[string]any{
		"error":  err.Error(),
		"detail": detail,
	})
}

func summarizeDiffStat(stat string) string {
	if strings.TrimSpace(stat) == "" {
		return ""
	}
	lines := strings.Split(strings.TrimSpace(stat), "\n")
	return lines[len(lines)-1]
}

type logChannelSink struct {
	ch chan<- string
}

func (s logChannelSink) PushLine(line string) {
	if s.ch == nil {
		return
	}
	select {
	case s.ch <- line:
	default:
	}
}

func listenSpecsLogs(logCh <-chan string, runID int) tea.Cmd {
	return func() tea.Msg {
		return specsLogBatchMsg{batch: drainLogChannel(runID, logCh, 64)}
	}
}
