// Package tui provides the Build Specs screen for the Ralph TUI.
// Entrypoint: specsView.
package tui

import (
	"container/list"
	"context"
	"errors"
	"fmt"
	"hash/fnv"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/glamour"
	"github.com/charmbracelet/huh"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
)

type specsView struct {
	cfg                     config.Config
	keys                    keyMap
	locations               paths.Locations
	runner                  specs.Runner
	runnerArgs              []string
	reasoningEffort         string
	interactive             bool
	innovate                bool
	innovateExplicit        bool
	autofillScout           bool
	autofillExplicit        bool
	scoutWorkflow           bool
	scoutExplicit           bool
	userFocus               string
	userFocusExplicit       bool
	editUserFocus           bool
	userFocusDraft          string
	userFocusForm           *huh.Form
	userFocusFormH          int
	effectiveInnovate       bool
	autoEnabled             bool
	autoEnabledReason       string
	preview                 string
	previewErr              string
	previewLoading          bool
	previewDirty            bool
	previewSignature        string
	lastResizeAt            time.Time
	resizeDebounce          time.Duration
	previewDebounceGen      int
	status                  string
	err                     string
	refreshErr              string
	persistErr              string
	previewViewport         viewport.Model
	logViewport             viewport.Model
	width                   int
	height                  int
	previewWidth            int
	rendererBuilder         func(int) (previewRenderer, error)
	previewRenderers        map[int]previewRenderer
	previewRendererLRU      *list.List
	previewRendererLRUIndex map[int]*list.Element
	running                 bool
	diffStat                string
	diffStatSummary         string
	diffStatSignature       string
	runLogBuf               logLineBuffer
	lastRunOutput           string
	lastRunSignature        logBufferSignature
	parentCtx               context.Context
	buildCancel             context.CancelFunc
	buildDone               chan struct{}
	logCh                   chan string
	logRunID                int
	pendingResult           *specsBuildResultMsg
	queueStamp              fileStamp
	promptStamp             fileStamp
	logger                  *tuiLogger
	output                  *outputFileWriter
	lastConfigAutofillScout bool
	lastConfigScoutWorkflow bool
	lastConfigUserFocus     string
	pendingLogViewportLines int
	lastLogViewportVersion  uint64
	lastLogViewportFlush    time.Time
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
	preview    string
	err        error
	effective  bool
	auto       bool
	autoReason string
	signature  string
	unchanged  bool
}

type specsPreviewDebounceMsg struct {
	gen int
}

func newSpecsView(cfg config.Config, locations paths.Locations, keys keyMap) (*specsView, error) {
	vp := viewport.New(80, 20)
	logViewport := viewport.New(80, 20)
	vp.Style = paddedViewportStyle
	logViewport.Style = paddedViewportStyle
	view := &specsView{
		cfg:                     cfg,
		keys:                    keys,
		locations:               locations,
		parentCtx:               context.Background(),
		runner:                  specs.Runner(cfg.Specs.Runner),
		runnerArgs:              cfg.Specs.RunnerArgs,
		reasoningEffort:         cfg.Specs.ReasoningEffort,
		autofillScout:           cfg.Specs.AutofillScout,
		scoutWorkflow:           cfg.Specs.ScoutWorkflow,
		userFocus:               cfg.Specs.UserFocus,
		lastConfigAutofillScout: cfg.Specs.AutofillScout,
		lastConfigScoutWorkflow: cfg.Specs.ScoutWorkflow,
		lastConfigUserFocus:     cfg.Specs.UserFocus,
		previewViewport:         vp,
		logViewport:             logViewport,
		previewWidth:            80,
		previewDirty:            true,
		resizeDebounce:          250 * time.Millisecond,
		rendererBuilder:         buildRenderer,
		previewRenderers:        map[int]previewRenderer{},
		previewRendererLRU:      list.New(),
		previewRendererLRUIndex: map[int]*list.Element{},
		runLogBuf:               newLogLineBuffer(500, 400),
	}
	if stamp, err := getFileStamp(filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")); err == nil {
		view.queueStamp = stamp
	}
	if promptPath, err := specs.ResolvePromptTemplate(cfg.Paths.PinDir, cfg.ProjectType, ""); err == nil {
		if stamp, err := getFileStamp(promptPath); err == nil {
			view.promptStamp = stamp
		}
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
			s.flushRunLogViewport(s.logViewport.AtBottom())
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
				return s.refreshPreviewIfDirty()
			}
			return nil
		}
		if msg.unchanged {
			s.previewSignature = msg.signature
			s.effectiveInnovate = msg.effective
			s.autoEnabled = msg.auto
			s.autoEnabledReason = msg.autoReason
			if s.previewDirty && !s.running {
				return s.refreshPreviewIfDirty()
			}
			return nil
		}
		s.previewErr = ""
		s.clearRefreshError()
		s.preview = msg.preview
		s.effectiveInnovate = msg.effective
		s.autoEnabled = msg.auto
		s.autoEnabledReason = msg.autoReason
		s.previewSignature = msg.signature
		s.previewViewport.SetContent(msg.preview)
		s.previewViewport.GotoTop()
		if s.previewDirty && !s.running {
			return s.refreshPreviewIfDirty()
		}
		return nil
	case specsPreviewDebounceMsg:
		if msg.gen != s.previewDebounceGen {
			return nil
		}
		if s.previewDirty && !s.running {
			return s.refreshPreviewIfDirty()
		}
		return nil
	case tea.KeyMsg:
		if s.editUserFocus && !s.running {
			switch msg.String() {
			case "esc":
				s.cancelUserFocusEditor()
				return nil
			case "ctrl+s":
				return s.commitUserFocusEditor()
			}
			if s.userFocusForm != nil {
				updated, cmd := s.userFocusForm.Update(msg)
				if form, ok := updated.(*huh.Form); ok {
					s.userFocusForm = form
				}
				return cmd
			}
			return nil
		}
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
		case key.Matches(msg, keys.ToggleScoutWorkflow) && !s.running:
			s.scoutWorkflow = !s.scoutWorkflow
			s.scoutExplicit = true
			return s.requestPreviewRefresh()
		case key.Matches(msg, keys.EditUserFocus) && !s.running:
			s.openUserFocusEditor()
			return nil
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

func (s *specsView) StartBuild() tea.Cmd {
	if s == nil || s.running {
		return nil
	}
	s.running = true
	s.status = "Running specs build..."
	s.err = ""
	s.persistErr = ""
	return s.runBuildCmd()
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
	innovate := yesNo(s.innovate)
	effectiveInnovate := yesNo(s.effectiveInnovate)
	innovateLine := fmt.Sprintf("Innovate: %s", innovate)
	if s.effectiveInnovate != s.innovate {
		innovateLine = fmt.Sprintf("Innovate: %s (effective: %s)", innovate, effectiveInnovate)
	}
	if s.autoEnabled {
		reason := "auto"
		if s.autoEnabledReason != "" {
			reason = "auto: " + s.autoEnabledReason
		}
		innovateLine = innovateLine + " (" + reason + ")"
	}
	userFocusLine := fmt.Sprintf("User focus: %s", formatUserFocus(s.userFocus))
	if s.editUserFocus {
		userFocusLine = "User focus: (editing...)"
	}
	effortResult := runnerargs.ApplyReasoningEffort(string(s.runner), s.runnerArgs, s.reasoningEffort)
	effectiveLabel := runnerargs.DisplayEffortResult(effortResult)
	lines := []string{
		fmt.Sprintf("Runner: %s", s.runner),
		fmt.Sprintf("Runner args: %d", len(s.runnerArgs)),
		fmt.Sprintf("Reasoning effort: %s (effective: %s)", runnerargs.DisplayEffort(s.reasoningEffort), effectiveLabel),
		fmt.Sprintf("Interactive: %s", yesNo(s.interactive)),
		innovateLine,
		fmt.Sprintf("Autofill scout: %s", yesNo(s.autofillScout)),
		fmt.Sprintf("Scout workflow: %s", yesNo(s.scoutWorkflow)),
		userFocusLine,
		renderKeyHints("Keys", specsKeyHintBindings(s.keys, s.running)),
		"Scroll: \u2191/\u2193 PgUp/PgDn (Ctrl+F to focus)",
	}
	if s.editUserFocus {
		lines = append(lines, "Edit focus: Esc cancel | Ctrl+S save")
	}
	return strings.Join(lines, "\n")
}

func (s *specsView) openUserFocusEditor() {
	s.userFocusDraft = s.userFocus
	s.userFocusForm = huh.NewForm(
		huh.NewGroup(
			huh.NewText().
				Title("Specs User Focus").
				Value(&s.userFocusDraft).
				Lines(s.userFocusEditorLines()).
				Key("specs.user_focus"),
		),
	).WithShowHelp(false)
	s.editUserFocus = true
	s.resizeUserFocusEditor(s.width, s.userFocusFormH)
}

func (s *specsView) userFocusEditorLines() int {
	const defaultLines = 8
	if s.userFocusFormH <= 0 {
		return defaultLines
	}
	lines := s.userFocusFormH - 4
	lines = max(lines, 3)
	lines = min(lines, 12)
	return lines
}

func (s *specsView) cancelUserFocusEditor() {
	s.editUserFocus = false
	s.userFocusForm = nil
	s.userFocusDraft = ""
}

func (s *specsView) commitUserFocusEditor() tea.Cmd {
	s.editUserFocus = false
	s.userFocus = strings.TrimSpace(s.userFocusDraft)
	s.userFocusExplicit = true
	s.userFocusForm = nil
	s.userFocusDraft = ""
	return tea.Batch(
		func() tea.Msg {
			return specsUserFocusUpdatedMsg{UserFocus: s.userFocus}
		},
		s.requestPreviewRefresh(),
	)
}

func (s *specsView) userFocusEditorView() string {
	lines := []string{
		"Editing User Focus",
		"Esc cancel | Ctrl+S save",
		"",
	}
	if s.userFocusForm != nil {
		lines = append(lines, s.userFocusForm.View())
	}
	return strings.Join(lines, "\n")
}

func (s *specsView) resizeUserFocusEditor(width int, editorHeight int) {
	if s.userFocusForm == nil {
		return
	}
	s.userFocusForm = resizeHuhFormToFit(s.userFocusForm, max(0, width), max(0, editorHeight))
}

func (s *specsView) previewView() string {
	if s.running {
		return s.logViewport.View()
	}
	if s.editUserFocus {
		return s.userFocusEditorView()
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
	scoutWorkflow := s.scoutWorkflow
	userFocus := s.userFocus
	lastRunOutput := s.lastRunOutput
	runSignature := s.lastRunSignature
	diffStat := s.diffStat
	diffStatSignature := s.diffStatSignature
	previewWidth := s.previewWidth
	promptStamp := s.promptStamp
	queueStamp := s.queueStamp
	priorSignature := s.previewSignature
	priorEffective := s.effectiveInnovate
	priorAuto := s.autoEnabled
	priorAutoReason := s.autoEnabledReason
	logger := s.logger

	var overrides specs.BuildOptionOverrides
	if innovateExplicit {
		overrides.Innovate = &innovate
	}
	if s.autofillExplicit {
		overrides.AutofillScout = &autofillScout
	}
	if s.scoutExplicit {
		overrides.ScoutWorkflow = &scoutWorkflow
	}
	if s.userFocusExplicit {
		overrides.UserFocus = &userFocus
	}
	resolved := specs.ResolveBuildOptions(
		specs.BuildOptionDefaults{
			Innovate:        innovate,
			AutofillScout:   cfg.Specs.AutofillScout,
			ScoutWorkflow:   cfg.Specs.ScoutWorkflow,
			UserFocus:       cfg.Specs.UserFocus,
			Runner:          s.runner,
			RunnerArgs:      s.runnerArgs,
			ReasoningEffort: s.reasoningEffort,
		},
		overrides,
		nil,
	)

	signature := previewInputSignature(previewWidth, promptStamp, queueStamp, interactive, resolved.Innovate, resolved.InnovateExplicit, resolved.AutofillScout, resolved.ScoutWorkflow, resolved.UserFocus, runSignature, diffStatSignature)
	if s.previewErr == "" && s.preview != "" && signature == priorSignature {
		return func() tea.Msg {
			return specsPreviewMsg{
				signature:  signature,
				effective:  priorEffective,
				auto:       priorAuto,
				autoReason: priorAutoReason,
				unchanged:  true,
			}
		}
	}
	renderer, err := s.previewRenderer(previewWidth)
	if err != nil {
		return func() tea.Msg {
			return specsPreviewMsg{err: err}
		}
	}
	return func() tea.Msg {
		start := time.Now()
		errNote := ""
		defer func() {
			if logger == nil {
				return
			}
			fields := map[string]any{
				"duration_ms": time.Since(start).Milliseconds(),
				"width":       previewWidth,
				"signature":   signature,
			}
			if errNote != "" {
				fields["error"] = errNote
			}
			logger.Debug("specs.preview.render", fields)
		}()
		queuePath := filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")
		resolution, err := specs.ResolveInnovateDetails(queuePath, resolved.Innovate, resolved.InnovateExplicit, resolved.AutofillScout)
		if err != nil {
			errNote = err.Error()
			return specsPreviewMsg{err: err}
		}

		promptPath, err := specs.ResolvePromptTemplate(cfg.Paths.PinDir, cfg.ProjectType, "")
		if err != nil {
			errNote = err.Error()
			return specsPreviewMsg{err: err}
		}
		prompt, err := specs.FillPrompt(promptPath, specs.FillPromptOptions{
			Interactive:   interactive,
			Innovate:      resolution.Effective,
			ScoutWorkflow: resolved.ScoutWorkflow,
			UserFocus:     resolved.UserFocus,
			ProjectType:   cfg.ProjectType,
		})
		if err != nil {
			errNote = err.Error()
			return specsPreviewMsg{err: err}
		}
		rendered, err := renderer.Render(prompt)
		if err != nil {
			errNote = err.Error()
			return specsPreviewMsg{err: err}
		}
		if lastRunOutput != "" {
			rendered = rendered + "\n\nBuild output:\n" + lastRunOutput
		}
		if diffStat != "" {
			rendered = rendered + "\n\nDiff stat:\n" + diffStat
		}
		return specsPreviewMsg{
			preview:    rendered,
			effective:  resolution.Effective,
			auto:       resolution.AutoEnabled,
			autoReason: resolution.AutoReason,
			signature:  signature,
		}
	}
}

func (s *specsView) requestPreviewRefresh() tea.Cmd {
	if s.previewLoading {
		s.previewDirty = true
		return nil
	}
	return s.refreshPreviewAsync()
}

func (s *specsView) debouncePreviewCmd(delay time.Duration) tea.Cmd {
	if s == nil {
		return nil
	}
	s.previewDebounceGen++
	gen := s.previewDebounceGen
	if delay <= 0 {
		return func() tea.Msg {
			return specsPreviewDebounceMsg{gen: gen}
		}
	}
	return tea.Tick(delay, func(time.Time) tea.Msg {
		return specsPreviewDebounceMsg{gen: gen}
	})
}

func (s *specsView) DebouncedRefreshPreviewCmd() tea.Cmd {
	if s == nil || s.running || s.editUserFocus {
		return nil
	}
	if !s.previewDirty || s.previewLoading {
		return nil
	}
	if s.resizeDebounce <= 0 {
		return s.debouncePreviewCmd(0)
	}
	return s.debouncePreviewCmd(s.resizeDebounce)
}

func (s *specsView) refreshPreviewIfDirty() tea.Cmd {
	if s == nil || s.running || s.editUserFocus {
		return nil
	}
	if !s.previewDirty {
		return nil
	}
	if s.previewLoading {
		s.previewDirty = true
		return nil
	}
	if s.resizeDebounce > 0 && !s.lastResizeAt.IsZero() {
		remaining := s.resizeDebounce - time.Since(s.lastResizeAt)
		if remaining > 0 {
			return s.debouncePreviewCmd(remaining)
		}
	}
	return s.refreshPreviewAsync()
}

func (s *specsView) runBuildCmd() tea.Cmd {
	s.resetRunLogs()
	s.startPersistingOutput()
	logCh := newLogChannel()
	s.logCh = logCh
	s.logRunID++
	runID := s.logRunID
	s.pendingResult = nil
	s.logViewport.SetContent("")
	s.logViewport.GotoTop()
	parentCtx := s.parentCtx
	if parentCtx == nil {
		parentCtx = context.Background()
	}
	ctx, cancel := context.WithCancel(parentCtx)
	s.buildCancel = cancel
	done := make(chan struct{})
	s.buildDone = done
	cfg := s.cfg
	innovate := s.innovate
	autofillScout := s.autofillScout
	scoutWorkflow := s.scoutWorkflow
	userFocus := s.userFocus
	var overrides specs.BuildOptionOverrides
	if s.innovateExplicit {
		overrides.Innovate = &innovate
	}
	if s.autofillExplicit {
		overrides.AutofillScout = &autofillScout
	}
	if s.scoutExplicit {
		overrides.ScoutWorkflow = &scoutWorkflow
	}
	if s.userFocusExplicit {
		overrides.UserFocus = &userFocus
	}
	resolved := specs.ResolveBuildOptions(
		specs.BuildOptionDefaults{
			Innovate:        innovate,
			AutofillScout:   cfg.Specs.AutofillScout,
			ScoutWorkflow:   cfg.Specs.ScoutWorkflow,
			UserFocus:       cfg.Specs.UserFocus,
			Runner:          s.runner,
			RunnerArgs:      s.runnerArgs,
			ReasoningEffort: s.reasoningEffort,
		},
		overrides,
		nil,
	)
	if s.logger != nil {
		s.logger.Info("specs.run.start", map[string]any{
			"runner":            resolved.Runner,
			"runner_args_count": len(resolved.RunnerArgs),
			"reasoning_effort":  runnerargs.DisplayEffort(resolved.ReasoningEffort),
			"interactive":       s.interactive,
			"innovate":          resolved.Innovate,
			"autofillScout":     resolved.AutofillScout,
			"scout_workflow":    resolved.ScoutWorkflow,
			"user_focus_set":    resolved.UserFocus != "",
		})
	}

	sink := logChannelSink{ch: logCh}
	writer := newStreamWriter(sink, s.cfg.Logging.MaxBufferedBytes)

	runCmd := func() tea.Msg {
		defer close(done)
		defer cancel()
		defer close(logCh)
		result, err := specs.Build(ctx, specs.BuildOptions{
			RepoRoot:         s.locations.RepoRoot,
			PinDir:           s.cfg.Paths.PinDir,
			ProjectType:      s.cfg.ProjectType,
			Runner:           resolved.Runner,
			RunnerArgs:       resolved.RunnerArgs,
			Interactive:      s.interactive,
			Innovate:         resolved.Innovate,
			InnovateExplicit: resolved.InnovateExplicit,
			AutofillScout:    resolved.AutofillScout,
			ScoutWorkflow:    resolved.ScoutWorkflow,
			UserFocus:        resolved.UserFocus,
			Stdout:           writer,
			Stderr:           writer,
		})
		writer.Flush()
		if err != nil {
			return specsBuildResultMsg{err: err}
		}
		files := pin.ResolveFiles(s.cfg.Paths.PinDir)
		pinErr := pin.ValidatePin(files, s.cfg.ProjectType)
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

func formatUserFocus(value string) string {
	trimmed := strings.TrimSpace(value)
	if trimmed == "" {
		return "(none)"
	}
	trimmed = strings.ReplaceAll(trimmed, "\n", " ")
	trimmed = strings.ReplaceAll(trimmed, "\r", " ")
	runes := []rune(trimmed)
	const maxRunes = 60
	if len(runes) <= maxRunes {
		return trimmed
	}
	return string(runes[:maxRunes-3]) + "..."
}

func (s *specsView) Resize(width int, height int) {
	s.width = width
	s.height = height
	header := "Build Specs"
	status := s.statusLine()
	options := s.optionsView()
	chrome := chromeHeight(
		width,
		wrappedBlock{Text: header, MinRows: 1},
		wrappedBlock{Text: status, MinRows: 1, BlankLinesAfter: 1},
		wrappedBlock{Text: options, MinRows: 1, BlankLinesAfter: 1},
	)
	previewHeight := remainingHeight(height, chrome)
	s.userFocusFormH = previewHeight
	if s.editUserFocus {
		s.resizeUserFocusEditor(width, previewHeight)
	}
	resizeViewportToFit(&s.previewViewport, max(0, width), max(0, previewHeight), paddedViewportStyle)
	resizeViewportToFit(&s.logViewport, max(0, width), max(0, previewHeight), paddedViewportStyle)
	s.previewWidth = max(0, s.previewViewport.Width)
	s.lastResizeAt = time.Now()
	s.previewDirty = true
}

type previewRenderer interface {
	Render(string) (string, error)
}

const specsPreviewRendererCacheMaxEntries = 8

func buildRenderer(previewWidth int) (previewRenderer, error) {
	wrapWidth := previewWidth
	if wrapWidth <= 0 {
		wrapWidth = 80
	}
	return glamour.NewTermRenderer(
		glamour.WithAutoStyle(),
		glamour.WithWordWrap(wrapWidth),
	)
}

func (s *specsView) clearPreviewRendererCache() {
	s.previewRenderers = map[int]previewRenderer{}
	s.previewRendererLRU = list.New()
	s.previewRendererLRUIndex = map[int]*list.Element{}
}

func (s *specsView) previewRenderer(width int) (previewRenderer, error) {
	if s.previewRenderers == nil {
		s.previewRenderers = map[int]previewRenderer{}
	}
	if s.previewRendererLRU == nil {
		s.previewRendererLRU = list.New()
	}
	if s.previewRendererLRUIndex == nil {
		s.previewRendererLRUIndex = map[int]*list.Element{}
	}
	if renderer, ok := s.previewRenderers[width]; ok {
		if element, ok := s.previewRendererLRUIndex[width]; ok {
			s.previewRendererLRU.MoveToFront(element)
		} else {
			s.previewRendererLRUIndex[width] = s.previewRendererLRU.PushFront(width)
		}
		return renderer, nil
	}
	if s.rendererBuilder == nil {
		s.rendererBuilder = buildRenderer
	}
	renderer, err := s.rendererBuilder(width)
	if err != nil {
		return nil, err
	}
	s.previewRenderers[width] = renderer
	s.previewRendererLRUIndex[width] = s.previewRendererLRU.PushFront(width)
	for len(s.previewRenderers) > specsPreviewRendererCacheMaxEntries {
		element := s.previewRendererLRU.Back()
		if element == nil {
			break
		}
		lruWidth, ok := element.Value.(int)
		if !ok {
			s.previewRendererLRU.Remove(element)
			continue
		}
		s.previewRendererLRU.Remove(element)
		delete(s.previewRendererLRUIndex, lruWidth)
		delete(s.previewRenderers, lruWidth)
	}
	return renderer, nil
}

func previewInputSignature(
	previewWidth int,
	promptStamp fileStamp,
	queueStamp fileStamp,
	interactive bool,
	innovate bool,
	innovateExplicit bool,
	autofillScout bool,
	scoutWorkflow bool,
	userFocus string,
	runSignature logBufferSignature,
	diffStatSignature string,
) string {
	hash := fnv.New64a()
	_, _ = fmt.Fprintf(hash, "width=%d;", previewWidth)
	_, _ = fmt.Fprintf(hash, "prompt=%s;", fileStampSignature(promptStamp))
	_, _ = fmt.Fprintf(hash, "queue=%s;", fileStampSignature(queueStamp))
	_, _ = fmt.Fprintf(hash, "interactive=%t;innovate=%t;innovateExplicit=%t;autofillScout=%t;", interactive, innovate, innovateExplicit, autofillScout)
	_, _ = fmt.Fprintf(hash, "scout=%t;focus=%s;", scoutWorkflow, userFocus)
	_, _ = fmt.Fprintf(hash, "run_version=%d;run_lines=%d;run_bytes=%d;", runSignature.version, runSignature.lineCount, runSignature.byteCount)
	_, _ = fmt.Fprintf(hash, "diff=%s;", diffStatSignature)
	return fmt.Sprintf("%x", hash.Sum(nil))
}

func (s *specsView) SetConfig(cfg config.Config, locations paths.Locations) {
	oldTheme := s.cfg.UI.Theme
	s.cfg = cfg
	s.locations = locations
	if s.lastConfigAutofillScout != cfg.Specs.AutofillScout {
		s.autofillScout = cfg.Specs.AutofillScout
		s.autofillExplicit = false
	} else if !s.autofillExplicit {
		s.autofillScout = cfg.Specs.AutofillScout
	}
	if s.lastConfigScoutWorkflow != cfg.Specs.ScoutWorkflow {
		s.scoutWorkflow = cfg.Specs.ScoutWorkflow
		s.scoutExplicit = false
	} else if !s.scoutExplicit {
		s.scoutWorkflow = cfg.Specs.ScoutWorkflow
	}
	if s.lastConfigUserFocus != cfg.Specs.UserFocus {
		s.userFocus = cfg.Specs.UserFocus
		s.userFocusExplicit = false
	} else if !s.userFocusExplicit {
		s.userFocus = cfg.Specs.UserFocus
	}
	s.lastConfigAutofillScout = cfg.Specs.AutofillScout
	s.lastConfigScoutWorkflow = cfg.Specs.ScoutWorkflow
	s.lastConfigUserFocus = cfg.Specs.UserFocus
	if !s.running {
		s.runner = specs.Runner(cfg.Specs.Runner)
		s.runnerArgs = cfg.Specs.RunnerArgs
		s.reasoningEffort = cfg.Specs.ReasoningEffort
	}
	if stamp, err := getFileStamp(filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")); err == nil {
		s.queueStamp = stamp
	}
	if promptPath, err := specs.ResolvePromptTemplate(cfg.Paths.PinDir, cfg.ProjectType, ""); err == nil {
		if stamp, err := getFileStamp(promptPath); err == nil {
			s.promptStamp = stamp
		}
	}
	if oldTheme != cfg.UI.Theme {
		s.clearPreviewRendererCache()
		s.previewDirty = true
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
		return s.refreshPreviewIfDirty()
	}
	queuePath := filepath.Join(s.cfg.Paths.PinDir, "implementation_queue.md")
	queueStamp, queueChanged, queueErr := fileChanged(queuePath, s.queueStamp)
	if queueErr != nil {
		s.setRefreshError("watch implementation_queue.md", queueErr)
		return nil
	}
	promptPath, err := specs.ResolvePromptTemplate(s.cfg.Paths.PinDir, s.cfg.ProjectType, "")
	if err != nil {
		s.setRefreshError("resolve prompt template", err)
		return nil
	}
	promptStamp, promptChanged, promptErr := fileChanged(promptPath, s.promptStamp)
	if promptErr != nil {
		s.setRefreshError("watch "+filepath.Base(promptPath), promptErr)
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
		s.previewDirty = true
		return s.refreshPreviewIfDirty()
	}
	return nil
}

func (s *specsView) RefreshPreviewCmd() tea.Cmd {
	if s.previewDirty {
		return s.refreshPreviewIfDirty()
	}
	return nil
}

func (s *specsView) Focus() {}

func (s *specsView) Blur() {}

func (s *specsView) IsTyping() bool {
	if s == nil {
		return false
	}
	return s.editUserFocus
}

func (s *specsView) activeViewport() *viewport.Model {
	if s.running {
		return &s.logViewport
	}
	return &s.previewViewport
}

func (s *specsView) resetRunLogs() {
	s.runLogBuf.Reset()
	s.lastRunOutput = ""
	s.diffStat = ""
	s.diffStatSummary = ""
	s.diffStatSignature = ""
	s.lastRunSignature = s.runLogBuf.Signature()
	s.pendingLogViewportLines = 0
	s.lastLogViewportVersion = 0
	s.lastLogViewportFlush = time.Time{}
}

func (s *specsView) appendRunLog(line string) {
	s.appendRunLogs([]string{line})
}

func (s *specsView) appendRunLogs(lines []string) {
	if len(lines) == 0 {
		return
	}
	s.persistSpecsLines(lines)
	atBottom := s.logViewport.AtBottom()
	s.runLogBuf.AppendLines(lines)
	s.pendingLogViewportLines += len(lines)
	if s.running {
		threshold := specsLogFlushThreshold(atBottom)
		if !shouldFlushLogViewport(s.pendingLogViewportLines, threshold, s.lastLogViewportFlush) {
			return
		}
	}
	s.flushRunLogViewport(atBottom)
}

func (s *specsView) finalizeRunOutput() {
	s.lastRunOutput = s.runLogBuf.ContentString()
	s.lastRunSignature = s.runLogBuf.Signature()
}

func (s *specsView) RunLogLines() []string {
	return s.runLogBuf.Lines()
}

func (s *specsView) flushRunLogViewport(wasAtBottom bool) {
	version := s.runLogBuf.Version()
	if version == s.lastLogViewportVersion {
		return
	}
	s.logViewport.SetContent(s.runLogBuf.ContentString())
	s.lastLogViewportVersion = version
	s.pendingLogViewportLines = 0
	s.lastLogViewportFlush = time.Now()
	if wasAtBottom {
		s.logViewport.GotoBottom()
	}
}

func specsLogFlushThreshold(atBottom bool) int {
	if atBottom {
		return 16
	}
	return 128
}

func (s *specsView) applyBuildResult(msg specsBuildResultMsg) tea.Cmd {
	s.buildCancel = nil
	s.buildDone = nil
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
	s.diffStatSummary = summarizeDiffStat(msg.diffStat)
	s.diffStatSignature = diffStatSignature(msg.diffStat)
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
	return specsOutputLogPath(s.cfg.Paths.CacheDir)
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

func diffStatSignature(stat string) string {
	if strings.TrimSpace(stat) == "" {
		return ""
	}
	hash := fnv.New64a()
	_, _ = hash.Write([]byte(stat))
	return fmt.Sprintf("len=%d;hash=%x", len(stat), hash.Sum(nil))
}

type logChannelSink struct {
	ch chan<- string
}

func (s logChannelSink) PushLine(line string) {
	sendLineBlocking(s.ch, line)
}

func listenSpecsLogs(logCh <-chan string, runID int) tea.Cmd {
	return func() tea.Msg {
		return specsLogBatchMsg{batch: drainLogChannel(runID, logCh, 64)}
	}
}
