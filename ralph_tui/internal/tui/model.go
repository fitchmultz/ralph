// Package tui provides the Bubble Tea model for the Ralph application shell.
// Entrypoint: Start.
package tui

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/help"
	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

// StartOptions provides override layers that should be preserved during reloads.
type StartOptions struct {
	CLIOverrides     config.PartialConfig
	SessionOverrides config.PartialConfig
}

// Start launches the TUI and blocks until it exits.
func Start(cfg config.Config, locations paths.Locations, opts StartOptions) error {
	program := tea.NewProgram(newModel(cfg, locations, opts), tea.WithAltScreen())
	finalModel, err := program.Run()
	switch m := finalModel.(type) {
	case model:
		m.Shutdown("program.exit")
		m.ShutdownWait(3 * time.Second)
	case *model:
		m.Shutdown("program.exit")
		m.ShutdownWait(3 * time.Second)
	}
	return err
}

type model struct {
	nav                 list.Model
	navBaseItems        []list.Item
	screen              screen
	help                help.Model
	keys                keyMap
	searchInput         textinput.Model
	searchActive        bool
	priorNavSelected    int
	searchNavCollapsed  bool
	navFocused          bool
	navCollapsed        bool
	cfg                 config.Config
	configView          *configEditor
	pinView             *pinView
	specsView           *specsView
	loopView            *loopView
	logsView            *logsView
	fixup               fixupState
	fixupLogCh          chan string
	fixupLogRunID       int
	fixupRunner         fixupRunner
	repoStatusSampler   *RepoStatusSampler
	repoStatus          repoStatusResult
	logger              *tuiLogger
	logErr              error
	runCtx              context.Context
	runCancel           context.CancelFunc
	shuttingDown        bool
	cliOverrides        config.PartialConfig
	sessionOverrides    config.PartialConfig
	refreshGen          int
	width               int
	height              int
	layout              layoutSpec
	initErr             error
	pinFixPrompt        *pinFixPrompt
	locations           paths.Locations
	loopAutoCollapsed   bool
	loopNavWasCollapsed bool
}

type focusedPanel int

const (
	focusedPanelNav focusedPanel = iota
	focusedPanelContent
)

func newModel(cfg config.Config, locations paths.Locations, opts StartOptions) model {
	navBaseItems := navItemsAsList()
	l := list.New(navBaseItems, list.NewDefaultDelegate(), 24, 16)
	l.Title = "Ralph"
	l.SetShowFilter(false)
	l.SetShowStatusBar(false)
	l.SetFilteringEnabled(false)
	l.SetShowHelp(false)

	searchInput := textinput.New()
	searchInput.Prompt = "Search: "
	searchInput.Placeholder = "Screens, queue IDs, tags"
	searchInput.CharLimit = 120
	searchInput.Width = 32

	var err error

	configView, configErr := newConfigEditor(locations, opts.CLIOverrides, opts.SessionOverrides)
	if err == nil {
		err = configErr
	}

	pinFiles := pin.ResolveFiles(cfg.Paths.PinDir)
	var pinFix *pinFixPrompt
	if err == nil {
		missing, missingErr := pin.MissingFiles(pinFiles, cfg.ProjectType)
		if missingErr != nil {
			err = missingErr
		} else if len(missing) > 0 {
			err = fmt.Errorf(
				"Ralph pin files missing:\n- %s\n\nRun `ralph init` to create defaults.",
				strings.Join(missing, "\n- "),
			)
		} else if pinErr := pin.ValidatePin(pinFiles, cfg.ProjectType); pinErr != nil {
			report, reportErr := pin.DuplicateIDs(pinFiles)
			if reportErr == nil && len(report.Fixable) > 0 && len(report.InDone) == 0 {
				pinFix = &pinFixPrompt{
					err:    pinErr,
					report: report,
				}
			} else {
				err = fmt.Errorf(
					"Ralph pin files are invalid: %v\n\nRun `ralph pin validate` for details or `ralph init --force` to reset defaults.",
					pinErr,
				)
			}
		}
	}

	pinView, pinErr := newPinView(cfg, locations)
	if err == nil {
		err = pinErr
	}

	keys := newKeyMap()
	specsView, specsErr := newSpecsView(cfg, locations, keys)
	if err == nil {
		err = specsErr
	}

	loopView := newLoopView(cfg, locations, keys)
	logsView := newLogsView("")
	runCtx, runCancel := context.WithCancel(context.Background())

	m := model{
		nav:               l,
		navBaseItems:      navBaseItems,
		screen:            screenDashboard,
		help:              help.New(),
		keys:              keys,
		searchInput:       searchInput,
		priorNavSelected:  l.Index(),
		navFocused:        true,
		navCollapsed:      false,
		cfg:               cfg,
		configView:        configView,
		pinView:           pinView,
		specsView:         specsView,
		loopView:          loopView,
		logsView:          logsView,
		runCtx:            runCtx,
		runCancel:         runCancel,
		cliOverrides:      opts.CLIOverrides,
		sessionOverrides:  opts.SessionOverrides,
		refreshGen:        1,
		initErr:           err,
		pinFixPrompt:      pinFix,
		locations:         locations,
		fixupRunner:       loop.FixupBlockedItems,
		repoStatusSampler: NewRepoStatusSampler(locations.RepoRoot, RepoStatusSamplerOptions{}),
	}
	if m.loopView != nil {
		m.loopView.parentCtx = m.runCtx
	}
	if m.specsView != nil {
		m.specsView.parentCtx = m.runCtx
	}
	m.setLogger(cfg)
	if m.logsView != nil {
		m.logsView.SetCacheDir(cfg.Paths.CacheDir)
		m.logsView.Refresh()
	}
	m.layout = computeLayoutWithBody(0, 0, m.navCollapsed)
	m.resizeViews(0, 0)
	m.applyFocus()
	return m
}

func (m model) Init() tea.Cmd {
	resolvedLogPath := ""
	if m.logger != nil {
		resolvedLogPath = m.logger.Path()
	} else if path, err := resolveLogPath(m.cfg); err == nil {
		resolvedLogPath = path
	}
	fields := map[string]any{
		"refresh_seconds":   m.cfg.UI.RefreshSeconds,
		"log_level":         m.cfg.Logging.Level,
		"log_file":          m.cfg.Logging.File,
		"resolved_log_path": resolvedLogPath,
	}
	if m.logErr != nil {
		fields["logger_error"] = m.logErr.Error()
	}
	m.logInfo("tui.start", fields)
	cmds := []tea.Cmd{refreshCmd(m.nextRefreshSeconds(), m.refreshGen)}
	if m.screen == screenDashboard {
		cmds = append(cmds, repoStatusCmd(m.runCtx, m.repoStatusSampler, false))
	}
	if m.pinView != nil {
		if cmd := m.pinView.reloadAsync(true); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	if m.specsView != nil && m.screen == screenBuildSpecs {
		if cmd := m.specsView.RefreshPreviewCmd(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	return tea.Batch(cmds...)
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd
	handled := false

	if m.pinFixPrompt != nil {
		switch msg := msg.(type) {
		case pinFixResultMsg:
			m.pinFixPrompt.running = false
			if msg.err != nil {
				m.initErr = msg.err
				m.pinFixPrompt = nil
				return m, nil
			}
			if m.pinView != nil {
				if err := m.pinView.reload(); err != nil {
					m.initErr = err
					m.pinFixPrompt = nil
					return m, nil
				}
			}
			m.pinFixPrompt = nil
			return m, nil
		case tea.KeyMsg:
			if m.pinFixPrompt.running {
				return m, nil
			}
			switch strings.ToLower(msg.String()) {
			case "y":
				m.pinFixPrompt.running = true
				pinFiles := pin.ResolveFiles(m.cfg.Paths.PinDir)
				return m, fixPinDuplicatesCmd(pinFiles, m.cfg.ProjectType)
			case "n", "q", "esc", "ctrl+c":
				m.initErr = fmt.Errorf(
					"Ralph pin files are invalid: %v\n\nRun `ralph pin validate` for details or `ralph pin fix-ids` to repair duplicates.",
					m.pinFixPrompt.err,
				)
				m.pinFixPrompt = nil
				return m, nil
			default:
				return m, nil
			}
		default:
			return m, nil
		}
	}

	switch msg := msg.(type) {
	case tea.InterruptMsg:
		m.logInfo("tui.interrupt", map[string]any{"screen": screenName(m.screen)})
		m.Shutdown("interrupt")
		return m, tea.Quit
	case tea.WindowSizeMsg:
		m.logDebug("window.resize", map[string]any{"width": msg.Width, "height": msg.Height})
		m.width = msg.Width
		m.height = msg.Height
		m.relayout()
		cmds = append(cmds, m.postResizeCmds()...)
		handled = true
	case refreshMsg:
		if msg.gen != m.refreshGen {
			m.logDebug("refresh.stale", map[string]any{"gen": msg.gen, "current_gen": m.refreshGen})
			handled = true
			break
		}
		m.logDebug("refresh.tick", map[string]any{"gen": msg.gen})
		cmds = append(cmds, m.refreshViews()...)
		cmds = append(cmds, refreshCmd(m.nextRefreshSeconds(), m.refreshGen))
		handled = true
	case loopRunModeMsg:
		m.applyLoopRunMode(msg.running)
		handled = true
	case fixupLogBatchMsg:
		if msg.batch.RunID != m.fixupLogRunID {
			handled = true
			break
		}
		if m.loopView != nil {
			m.loopView.appendLogLines(msg.batch.Lines)
		}
		if last := lastNonEmptyLineExcluding(msg.batch.Lines, isFixupSystemLine); last != "" {
			m.fixup.lastLogLine = last
		} else {
			fallback := lastNonEmptyLine(msg.batch.Lines)
			if fallback != "" && (m.fixup.lastLogLine == "" || isFixupSystemLine(m.fixup.lastLogLine)) {
				m.fixup.lastLogLine = fallback
			}
		}
		m.refreshLogsView()
		if msg.batch.Done {
			m.fixupLogCh = nil
			if !m.fixup.hasSummary && !m.fixup.running && !m.fixup.finishedAt.IsZero() {
				m.fixup.hasSummary = true
			}
		} else if m.fixupLogCh != nil {
			cmds = append(cmds, listenFixupLogs(m.fixupLogCh, m.fixupLogRunID))
		}
		handled = true
	case fixupResultMsg:
		if msg.runID != m.fixupLogRunID {
			handled = true
			break
		}
		m.fixup.running = false
		m.fixup.finishedAt = msg.finishedAt
		m.fixup.summary = summaryFromFixupResult(msg.result)
		if msg.err != nil {
			m.fixup.err = msg.err.Error()
		} else {
			m.fixup.err = ""
		}
		if m.fixupLogCh == nil {
			m.fixup.hasSummary = true
		}
		m.logInfo("fixup.stop", map[string]any{
			"scanned_blocked": m.fixup.summary.scanned,
			"eligible":        m.fixup.summary.eligible,
			"requeued":        m.fixup.summary.requeued,
			"skipped":         m.fixup.summary.skipped,
			"failed":          m.fixup.summary.failed,
			"error":           m.fixup.err,
		})
		handled = true
	case configReloadMsg:
		if msg.err != nil {
			m.logError("config.reload.error", msg.err)
			if m.configView != nil {
				m.configView.saveError = msg.err.Error()
				m.configView.saveNote = ""
			} else {
				m.initErr = msg.err
			}
			handled = true
			break
		}
		m.cfg = msg.cfg
		m.applyConfig()
		m.refreshGen++
		cmds = append(cmds, refreshCmd(m.nextRefreshSeconds(), m.refreshGen))
		m.logInfo("config.reload", map[string]any{
			"refresh_seconds": m.cfg.UI.RefreshSeconds,
			"log_level":       m.cfg.Logging.Level,
			"log_file":        m.cfg.Logging.File,
			"refresh_gen":     m.refreshGen,
		})
		handled = true
	case configReloadRequestMsg:
		m.syncSessionOverridesFromEditor()
		return m, m.reloadConfigCmd()
	case specsUserFocusUpdatedMsg:
		value := strings.TrimSpace(msg.UserFocus)
		if m.sessionOverrides.Specs == nil {
			m.sessionOverrides.Specs = &config.SpecsPartial{}
		}
		m.sessionOverrides.Specs.UserFocus = &value
		if m.configView != nil {
			m.configView.SetSessionOverrides(m.sessionOverrides)
		}
		return m, m.reloadConfigCmd()
	case repoStatusMsg:
		m.repoStatus = msg.result
		handled = true
	case tea.KeyMsg:
		keyFields := keyEventSummary(msg)
		keyFields["screen"] = screenName(m.screen)
		keyFields["nav_focused"] = m.navFocused
		keyFields["nav_collapsed"] = m.navCollapsed
		m.logDebug("key.event", keyFields)
		if isHardQuitKey(msg) {
			m.logInfo("tui.quit", map[string]any{"screen": screenName(m.screen)})
			m.Shutdown("key.quit")
			return m, tea.Quit
		}
		if key.Matches(msg, m.keys.Quit) {
			m.logInfo("tui.quit", map[string]any{"screen": screenName(m.screen)})
			m.Shutdown("key.quit")
			return m, tea.Quit
		}
		if m.searchActive {
			cmd := m.updateSearch(msg)
			return m, cmd
		}
		if m.isTyping() {
			cmd := m.updateActiveView(msg)
			return m, cmd
		}
		if key.Matches(msg, m.keys.ToggleNav) {
			m.navCollapsed = !m.navCollapsed
			if m.loopAutoCollapsed {
				m.loopAutoCollapsed = false
			}
			if m.navCollapsed {
				m.navFocused = false
			}
			m.applyFocus()
			m.relayout()
			return m, nil
		}
		if key.Matches(msg, m.keys.Help) {
			m.help.ShowAll = !m.help.ShowAll
			m.relayout()
			return m, nil
		}
		if key.Matches(msg, m.keys.RefreshNow) {
			cmds = append(cmds, m.refreshScreen(m.screen, true)...)
			return m, tea.Batch(cmds...)
		}
		if key.Matches(msg, m.keys.Search) {
			m.beginSearch()
			return m, nil
		}
		if key.Matches(msg, m.keys.JumpToPin) {
			cmds = append(cmds, m.switchScreen(screenPin, true)...)
			if m.pinView != nil && m.loopView != nil {
				m.pinView.SelectItemByID(m.loopView.ActiveItemID())
			}
			return m, tea.Batch(cmds...)
		}
		if key.Matches(msg, m.keys.JumpToLogs) {
			cmds = append(cmds, m.switchScreen(screenLogs, true)...)
			return m, tea.Batch(cmds...)
		}
		if m.screen == screenDashboard {
			switch {
			case key.Matches(msg, m.keys.DashboardRunLoopOnce):
				if m.loopView == nil {
					return m, nil
				}
				return m, m.loopView.StartOnce()
			case key.Matches(msg, m.keys.DashboardFixupBlocked):
				return m, m.startFixupCmd()
			case key.Matches(msg, m.keys.DashboardBuildSpecs):
				if m.specsView == nil {
					return m, nil
				}
				return m, m.specsView.StartBuild()
			}
		}
		if key.Matches(msg, m.keys.EditSpecsSettings) && m.screen == screenBuildSpecs {
			cmds = append(cmds, m.switchScreen(screenConfig, true)...)
			return m, tea.Batch(cmds...)
		}
		if m.screen == screenLogs && m.logsView != nil {
			switch {
			case key.Matches(msg, m.keys.ToggleLogsFormat):
				m.logsView.ToggleFormat()
				return m, nil
			case key.Matches(msg, m.keys.CycleLogsLevelFilter):
				m.logsView.CycleLevelFilter()
				return m, nil
			case key.Matches(msg, m.keys.CycleLogsComponentFilter):
				m.logsView.CycleComponentFilter()
				return m, nil
			case key.Matches(msg, m.keys.ClearLogsFilters):
				m.logsView.ClearFilters()
				return m, nil
			}
		}
		if key.Matches(msg, m.keys.Focus) {
			if m.navCollapsed {
				if m.navFocused {
					m.navFocused = false
					m.applyFocus()
					m.relayout()
				}
				return m, nil
			}
			m.navFocused = !m.navFocused
			m.applyFocus()
			m.relayout()
			return m, nil
		}
		if m.navFocused && !m.navCollapsed {
			updated, cmd := m.nav.Update(msg)
			m.nav = updated
			cmds = append(cmds, cmd)

			if key.Matches(msg, m.keys.Select) {
				if item, ok := m.nav.SelectedItem().(navItem); ok {
					cmds = append(cmds, m.switchScreen(item.screen, true)...)
				}
			}
		} else {
			cmds = append(cmds, m.updateActiveView(msg))
		}

		return m, tea.Batch(cmds...)
	}

	if !handled {
		if cmd, ok := m.updateBackgroundViews(msg); ok {
			if cmd != nil {
				cmds = append(cmds, cmd)
			}
		} else {
			cmds = append(cmds, m.updateActiveView(msg))
		}
	}

	return m, tea.Batch(cmds...)
}

type loopRunModeMsg struct {
	running bool
}

type fixupRunner func(context.Context, loop.FixupOptions) (loop.FixupResult, error)

type fixupFailureDetail struct {
	id     string
	reason string
}

type fixupSummary struct {
	scanned  int
	eligible int
	requeued int
	skipped  int
	failed   int
	lastID   string
	lastNote string
	failures []fixupFailureDetail
}

type fixupState struct {
	running     bool
	err         string
	summary     fixupSummary
	hasSummary  bool
	startedAt   time.Time
	finishedAt  time.Time
	lastLogLine string
}

type fixupResultMsg struct {
	runID      int
	result     loop.FixupResult
	err        error
	finishedAt time.Time
}

type fixupLogBatchMsg struct {
	batch logBatch
}

const (
	defaultFixupMaxAttempts = 3
	defaultFixupMaxItems    = 0
)

func loopRunModeCmd(running bool) tea.Cmd {
	return func() tea.Msg {
		return loopRunModeMsg{running: running}
	}
}

func shortFixupReason(reason string) string {
	trimmed := strings.TrimSpace(reason)
	if trimmed == "" {
		return ""
	}
	return loop.CommitMessageShort(trimmed)
}

func lastNonEmptyLine(lines []string) string {
	for i := len(lines) - 1; i >= 0; i-- {
		line := strings.TrimSpace(lines[i])
		if line != "" {
			return line
		}
	}
	return ""
}

func lastNonEmptyLineExcluding(lines []string, skip func(string) bool) string {
	for i := len(lines) - 1; i >= 0; i-- {
		line := strings.TrimSpace(lines[i])
		if line == "" {
			continue
		}
		if skip != nil && skip(line) {
			continue
		}
		return line
	}
	return ""
}

func isFixupSystemLine(line string) bool {
	return strings.Contains(line, "Fixup blocked starting.") ||
		strings.Contains(line, "Fixup blocked completed:") ||
		strings.Contains(line, "Fixup blocked failed:")
}

func summaryFromFixupResult(result loop.FixupResult) fixupSummary {
	var lastID string
	var lastNote string
	failures := make([]fixupFailureDetail, 0, len(result.FailedReasons))
	if len(result.FailedReasons) > 0 {
		last := result.FailedReasons[len(result.FailedReasons)-1]
		lastID = last.ID
		lastNote = shortFixupReason(last.Reason)
		for _, failure := range result.FailedReasons {
			failures = append(failures, fixupFailureDetail{
				id:     failure.ID,
				reason: shortFixupReason(failure.Reason),
			})
		}
	}
	if len(result.FailedIDs) > 0 {
		lastID = result.FailedIDs[len(result.FailedIDs)-1]
		if len(failures) == 0 {
			for _, id := range result.FailedIDs {
				failures = append(failures, fixupFailureDetail{id: id})
			}
		}
	}
	failed := len(result.FailedIDs)
	if len(result.FailedReasons) > failed {
		failed = len(result.FailedReasons)
	}
	return fixupSummary{
		scanned:  result.ScannedBlocked,
		eligible: result.Eligible,
		requeued: len(result.RequeuedIDs),
		skipped:  len(result.SkippedMax),
		failed:   failed,
		lastID:   lastID,
		lastNote: lastNote,
		failures: failures,
	}
}

func formatFixupSummary(summary fixupSummary) string {
	base := fmt.Sprintf(
		"Scanned %d | Eligible %d | Requeued %d | Skipped %d | Failed %d",
		summary.scanned,
		summary.eligible,
		summary.requeued,
		summary.skipped,
		summary.failed,
	)
	if summary.lastID == "" {
		return base
	}
	if summary.lastNote == "" {
		return fmt.Sprintf("%s | Last failed %s", base, summary.lastID)
	}
	return fmt.Sprintf("%s | Last failed %s: %s", base, summary.lastID, summary.lastNote)
}

func listenFixupLogs(logCh <-chan string, runID int) tea.Cmd {
	return func() tea.Msg {
		return fixupLogBatchMsg{batch: drainLogChannel(runID, logCh, 64)}
	}
}

func (m *model) startFixupCmd() tea.Cmd {
	if m.fixup.running {
		m.fixup.err = "Fixup already running"
		if m.loopView != nil {
			m.loopView.appendLogLine(">> [RALPH] Fixup blocked already running.")
		}
		m.refreshLogsView()
		return nil
	}

	m.fixup.running = true
	m.fixup.err = ""
	m.fixup.hasSummary = false
	m.fixup.lastLogLine = ""
	m.fixup.startedAt = time.Now()
	m.fixup.finishedAt = time.Time{}
	m.fixupLogRunID++
	runID := m.fixupLogRunID

	logCh := newLogChannel()
	m.fixupLogCh = logCh

	m.logInfo("fixup.start", map[string]any{
		"max_attempts": defaultFixupMaxAttempts,
		"max_items":    defaultFixupMaxItems,
	})

	fixupLogger := loopLogger{write: func(line string) {
		sendLineBlocking(logCh, line)
	}}

	runCmd := func() tea.Msg {
		defer close(logCh)
		if m.fixupRunner == nil {
			return fixupResultMsg{runID: runID, err: errors.New("fixup runner not configured"), finishedAt: time.Now()}
		}
		sendLineBlocking(logCh, ">> [RALPH] Fixup blocked starting.")
		result, err := m.fixupRunner(m.runCtx, loop.FixupOptions{
			RepoRoot:            m.locations.RepoRoot,
			PinDir:              m.cfg.Paths.PinDir,
			ProjectType:         m.cfg.ProjectType,
			MaxAttempts:         defaultFixupMaxAttempts,
			MaxItems:            defaultFixupMaxItems,
			RequireMain:         m.cfg.Loop.RequireMain,
			AutoCommit:          m.cfg.Git.AutoCommit,
			AutoPush:            m.cfg.Git.AutoPush,
			RedactionMode:       m.cfg.Logging.RedactionMode,
			LogMaxBufferedBytes: m.cfg.Logging.MaxBufferedBytes,
			Logger:              fixupLogger,
		})
		if err != nil {
			sendLineBlocking(logCh, ">> [RALPH] Fixup blocked failed: "+err.Error())
		} else {
			summary := summaryFromFixupResult(result)
			sendLineBlocking(logCh, ">> [RALPH] Fixup blocked completed: "+formatFixupSummary(summary))
		}
		return fixupResultMsg{runID: runID, result: result, err: err, finishedAt: time.Now()}
	}

	return tea.Batch(runCmd, listenFixupLogs(logCh, runID))
}

func (m model) isTyping() bool {
	if m.focusedPanelEffective() != focusedPanelContent {
		return false
	}
	switch m.screen {
	case screenConfig:
		return m.configView != nil && m.configView.IsTyping()
	case screenPin:
		return m.pinView != nil && m.pinView.IsTyping()
	case screenBuildSpecs:
		return m.specsView != nil && m.specsView.IsTyping()
	case screenRunLoop:
		return m.loopView != nil && m.loopView.IsTyping()
	default:
		return false
	}
}

// updateBackgroundViews ensures async view messages are handled even when inactive.
func (m *model) updateBackgroundViews(msg tea.Msg) (tea.Cmd, bool) {
	switch msg.(type) {
	case pinReloadMsg:
		if m.pinView == nil {
			return nil, true
		}
		return m.pinView.Update(msg, m.keys, m.loopMode()), true
	case specsBuildResultMsg, specsLogBatchMsg, specsPreviewMsg:
		if m.specsView == nil {
			return nil, true
		}
		return m.specsView.Update(msg, m.keys), true
	case loopResultMsg, loopLogBatchMsg, loopStateMsg:
		if m.loopView == nil {
			return nil, true
		}
		return m.loopView.Update(msg, m.keys), true
	default:
		return nil, false
	}
}

func (m model) View() string {
	if m.pinFixPrompt != nil {
		return pinFixPromptView(*m.pinFixPrompt)
	}
	if m.initErr != nil {
		return fmt.Sprintf("Error: %v\n", m.initErr)
	}

	navView := strings.TrimRight(m.nav.View(), "\n")
	if m.searchActive {
		searchView := m.searchView()
		if searchView != "" {
			navView = searchView + "\n" + navView
		}
	}
	contentView := strings.TrimRight(m.contentView(), "\n")

	navStyle, contentStyle := m.panelStyles(m.layout.navWidth, m.layout.bodyHeight, m.layout.contentWidth, m.layout.bodyHeight)
	navFrameW, navFrameH := navStyle.GetFrameSize()
	contentFrameW, contentFrameH := contentStyle.GetFrameSize()

	navInnerW := max(0, m.layout.navWidth-navFrameW)
	contentInnerW := max(0, m.layout.contentWidth-contentFrameW)
	navInnerH := max(0, m.layout.bodyHeight-navFrameH)
	contentInnerH := max(0, m.layout.bodyHeight-contentFrameH)
	navBorderW := navStyle.GetBorderLeftSize() + navStyle.GetBorderRightSize()
	navBorderH := navStyle.GetBorderTopSize() + navStyle.GetBorderBottomSize()
	contentBorderW := contentStyle.GetBorderLeftSize() + contentStyle.GetBorderRightSize()
	contentBorderH := contentStyle.GetBorderTopSize() + contentStyle.GetBorderBottomSize()
	navBoxW := max(0, m.layout.navWidth-navBorderW)
	navBoxH := max(0, m.layout.bodyHeight-navBorderH)
	contentBoxW := max(0, m.layout.contentWidth-contentBorderW)
	contentBoxH := max(0, m.layout.bodyHeight-contentBorderH)

	if navInnerW > 0 && navInnerH > 0 {
		navView = clampToSize(navView, navInnerW, navInnerH)
	} else {
		navView = ""
	}
	if contentInnerW > 0 && contentInnerH > 0 {
		contentView = clampToSize(contentView, contentInnerW, contentInnerH)
	} else {
		contentView = ""
	}

	navStyle = navStyle.Width(navBoxW).Height(navBoxH)
	contentStyle = contentStyle.Width(contentBoxW).Height(contentBoxH)

	left := navStyle.Render(navView)
	right := contentStyle.Render(contentView)
	body := lipgloss.JoinHorizontal(lipgloss.Top, left, right)
	body = strings.TrimRight(body, "\n")

	m.help.Width = max(0, m.width)
	footer := strings.TrimRight(m.help.View(m.helpKeyMap()), "\n")
	footer = clipToHeight(footer, m.layout.footerHeight)
	footer = clampToSize(footer, max(0, m.width), 0)
	rendered := body
	if m.layout.footerHeight > 0 {
		if m.layout.bodyHeight > 0 && rendered != "" {
			gap := m.layout.footerGap
			if gap <= 0 {
				gap = 1
			}
			rendered = rendered + strings.Repeat("\n", gap) + footer
		} else {
			rendered = footer
		}
	}
	return withFinalNewline(rendered)
}

func (m model) searchView() string {
	if !m.searchActive {
		return ""
	}
	return m.searchInput.View()
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func (m model) contentView() string {
	switch m.screen {
	case screenDashboard:
		return m.dashboardView()
	case screenRunLoop:
		if m.loopView == nil {
			return "Run Loop\n\nRun loop unavailable."
		}
		return m.loopView.View()
	case screenBuildSpecs:
		if m.specsView == nil {
			return "Build Specs\n\nSpecs builder unavailable."
		}
		return m.specsView.View()
	case screenPin:
		if m.pinView == nil {
			return "Pin\n\nPin view unavailable."
		}
		return m.pinView.View()
	case screenConfig:
		if m.configView == nil {
			return "Config\n\nConfig editor unavailable."
		}
		return m.configView.View()
	case screenLogs:
		if m.logsView == nil {
			return "Logs\n\nLogs view unavailable."
		}
		return m.logsView.View()
	case screenHelp:
		return "Help\n\nUse the left menu to navigate and Ctrl+F to switch focus."
	default:
		return ""
	}
}

type layoutSpec struct {
	navWidth     int
	contentWidth int
	bodyHeight   int
	footerHeight int
	footerGap    int
}

const (
	defaultNavWidth     = 26
	defaultContentWidth = 80
	minNavWidth         = 20
	minContentWidth     = 30
	footerGapBlankLines = 1
)

func computeLayoutWithBody(width int, bodyHeight int, navCollapsed bool) layoutSpec {
	if bodyHeight < 0 {
		bodyHeight = 0
	}
	if width <= 0 {
		return layoutSpec{
			navWidth:     0,
			contentWidth: 0,
			bodyHeight:   bodyHeight,
		}
	}
	if navCollapsed {
		return layoutSpec{
			navWidth:     0,
			contentWidth: width,
			bodyHeight:   bodyHeight,
		}
	}

	navWidth := defaultNavWidth
	maxNav := width / 3
	if width >= minNavWidth+minContentWidth {
		if maxNav < minNavWidth {
			maxNav = minNavWidth
		}
		if navWidth > maxNav {
			navWidth = maxNav
		}
	} else {
		navWidth = min(navWidth, maxNav)
		if navWidth < 0 {
			navWidth = 0
		}
	}

	contentWidth := width - navWidth
	if width >= minNavWidth+minContentWidth {
		if contentWidth < minContentWidth {
			navWidth = width - minContentWidth
			if navWidth < minNavWidth {
				navWidth = minNavWidth
			}
			contentWidth = width - navWidth
		}
	} else {
		targetNav := width - minContentWidth
		if targetNav < 0 {
			targetNav = 0
		}
		if maxNav > 0 && targetNav > maxNav {
			targetNav = maxNav
		}
		navWidth = min(navWidth, targetNav)
		if navWidth < 0 {
			navWidth = 0
		}
		if navWidth > width {
			navWidth = width
		}
		contentWidth = width - navWidth
		if contentWidth < 0 {
			contentWidth = 0
		}
	}

	return layoutSpec{
		navWidth:     navWidth,
		contentWidth: contentWidth,
		bodyHeight:   bodyHeight,
	}
}

func (m model) panelStyles(navOuterW, navOuterH, contentOuterW, contentOuterH int) (lipgloss.Style, lipgloss.Style) {
	border := lipgloss.RoundedBorder()
	focusedColor := lipgloss.AdaptiveColor{Light: "63", Dark: "75"}
	unfocusedColor := lipgloss.AdaptiveColor{Light: "245", Dark: "238"}

	panelStyleFor := func(outerW, outerH int, isFocused bool) lipgloss.Style {
		style := lipgloss.NewStyle()
		if outerW < 2 || outerH < 2 {
			return style
		}
		style = style.Border(border)
		if isFocused {
			style = style.BorderForeground(focusedColor)
		} else {
			style = style.BorderForeground(unfocusedColor)
		}
		paddingTop, paddingRight, paddingBottom, paddingLeft := 1, 1, 0, 1
		if outerW < 4 {
			paddingRight = 0
			paddingLeft = 0
		}
		if outerH <= 3 {
			paddingTop = 0
			paddingBottom = 0
		}
		return style.Padding(paddingTop, paddingRight, paddingBottom, paddingLeft)
	}

	navFocused := m.navPanelFocusedEffective()
	navStyle := panelStyleFor(navOuterW, navOuterH, navFocused)
	contentStyle := panelStyleFor(contentOuterW, contentOuterH, !navFocused)

	return navStyle, contentStyle
}

func (m *model) relayout() {
	height := max(0, m.height)
	m.help.Width = max(0, m.width)
	footer := strings.TrimRight(m.help.View(m.helpKeyMap()), "\n")
	rawFooterH := lipgloss.Height(footer)

	footerGap := 0
	if rawFooterH > 0 && height > 0 {
		footerGap = footerGapBlankLines + 1
		if height < rawFooterH+footerGap {
			if height >= rawFooterH+1 {
				footerGap = 1
			} else {
				footerGap = 0
			}
		}
	}
	footerHeight := min(rawFooterH, max(0, height-footerGap))
	bodyH := height - footerHeight - footerGap
	if bodyH < 0 {
		bodyH = 0
	}
	if bodyH == 0 {
		footerGap = 0
		footerHeight = min(rawFooterH, height)
		bodyH = height - footerHeight
	}

	m.layout = computeLayoutWithBody(m.width, bodyH, m.navCollapsed)
	m.layout.footerHeight = footerHeight
	m.layout.footerGap = footerGap

	navStyle, contentStyle := m.panelStyles(m.layout.navWidth, m.layout.bodyHeight, m.layout.contentWidth, m.layout.bodyHeight)
	navFrameW, navFrameH := navStyle.GetFrameSize()
	contentFrameW, contentFrameH := contentStyle.GetFrameSize()

	navInnerW := max(0, m.layout.navWidth-navFrameW)
	contentInnerW := max(0, m.layout.contentWidth-contentFrameW)
	navInnerH := max(0, m.layout.bodyHeight-navFrameH)
	contentInnerH := max(0, m.layout.bodyHeight-contentFrameH)

	m.nav.SetSize(navInnerW, navInnerH)
	promptWidth := lipgloss.Width(m.searchInput.Prompt)
	m.searchInput.Width = max(0, navInnerW-promptWidth)
	m.resizeViews(contentInnerW, contentInnerH)
}

func (m *model) resizeViews(contentInnerW int, contentInnerH int) {
	if m.configView != nil {
		m.configView.Resize(contentInnerW, contentInnerH)
	}
	if m.pinView != nil {
		m.pinView.Resize(contentInnerW, contentInnerH)
	}
	if m.specsView != nil {
		m.specsView.Resize(contentInnerW, contentInnerH)
	}
	if m.loopView != nil {
		m.loopView.Resize(contentInnerW, contentInnerH)
	}
	if m.logsView != nil {
		m.logsView.Resize(contentInnerW, contentInnerH)
	}
}

func (m *model) postResizeCmds() []tea.Cmd {
	cmds := make([]tea.Cmd, 0)
	if m.specsView != nil && m.screen == screenBuildSpecs {
		if cmd := m.specsView.DebouncedRefreshPreviewCmd(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	return cmds
}

type refreshMsg struct {
	gen int
}

type configReloadRequestMsg struct{}

type configReloadMsg struct {
	cfg config.Config
	err error
}

func refreshCmd(seconds int, gen int) tea.Cmd {
	if seconds <= 0 {
		return nil
	}
	return tea.Tick(time.Duration(seconds)*time.Second, func(time.Time) tea.Msg {
		return refreshMsg{gen: gen}
	})
}

func (m model) reloadConfigCmd() tea.Cmd {
	locations := m.locations
	cliOverrides := m.cliOverrides
	sessionOverrides := m.sessionOverrides
	return func() tea.Msg {
		cfg, err := config.LoadFromLocations(config.LoadOptions{
			Locations:        locations,
			CLIOverrides:     cliOverrides,
			SessionOverrides: sessionOverrides,
		})
		return configReloadMsg{cfg: cfg, err: err}
	}
}

func (m *model) syncSessionOverridesFromEditor() {
	if m.configView == nil {
		return
	}
	m.sessionOverrides = m.configView.SessionOverrides()
}

func (m *model) applyConfig() {
	if m.pinView != nil {
		if err := m.pinView.SetConfig(m.cfg, m.locations); err != nil {
			m.pinView.err = err.Error()
		}
	}
	if m.specsView != nil {
		m.specsView.SetConfig(m.cfg, m.locations)
	}
	if m.loopView != nil {
		m.loopView.SetConfig(m.cfg, m.locations)
	}
	m.setLogger(m.cfg)
	if m.logsView != nil {
		m.logsView.SetCacheDir(m.cfg.Paths.CacheDir)
		m.logsView.Refresh()
	}
}

func (m *model) refreshViews() []tea.Cmd {
	start := time.Now()
	cmds := make([]tea.Cmd, 0)
	switch m.screen {
	case screenDashboard:
		if m.pinView != nil {
			if cmd := m.pinView.RefreshIfNeeded(); cmd != nil {
				cmds = append(cmds, cmd)
			}
		}
		cmds = append(cmds, repoStatusCmd(m.runCtx, m.repoStatusSampler, false))
	case screenPin:
		if m.pinView != nil {
			if cmd := m.pinView.RefreshIfNeeded(); cmd != nil {
				cmds = append(cmds, cmd)
			}
		}
	case screenBuildSpecs:
		if m.specsView != nil {
			if cmd := m.specsView.RefreshIfNeeded(); cmd != nil {
				cmds = append(cmds, cmd)
			}
		}
	case screenLogs:
		m.refreshLogsView()
	}
	m.logDebug("refresh.views", map[string]any{
		"screen":      screenName(m.screen),
		"cmd_count":   len(cmds),
		"duration_ms": time.Since(start).Milliseconds(),
	})
	return cmds
}

func (m *model) refreshScreen(target screen, force bool) []tea.Cmd {
	cmds := make([]tea.Cmd, 0)
	switch target {
	case screenDashboard:
		cmds = append(cmds, repoStatusCmd(m.runCtx, m.repoStatusSampler, force))
	case screenPin:
		if m.pinView == nil {
			return cmds
		}
		if force {
			if m.pinView.mode != pinModeTable {
				return cmds
			}
			if cmd := m.pinView.reloadAsync(false); cmd != nil {
				cmds = append(cmds, cmd)
			}
			return cmds
		}
		if cmd := m.pinView.RefreshIfNeeded(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	case screenBuildSpecs:
		if m.specsView == nil || m.specsView.running {
			return cmds
		}
		if force {
			if cmd := m.specsView.requestPreviewRefresh(); cmd != nil {
				cmds = append(cmds, cmd)
			}
			return cmds
		}
		if cmd := m.specsView.RefreshIfNeeded(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	case screenLogs:
		m.refreshLogsView()
	}
	return cmds
}

func (m *model) refreshLogsView() {
	if m.logsView == nil {
		return
	}
	m.syncLoggerErrorToLogsView()
	m.logsView.Refresh()
}

func (m model) nextRefreshSeconds() int {
	base := m.cfg.UI.RefreshSeconds
	if base <= 0 {
		base = 1
	}
	multiplier := 1
	switch m.screen {
	case screenDashboard, screenPin:
		multiplier = 1
	case screenBuildSpecs:
		if m.specsView != nil && m.specsView.running {
			multiplier = 2
		}
	case screenRunLoop, screenLogs:
		multiplier = 2
	case screenConfig, screenHelp:
		multiplier = 4
	default:
		multiplier = 2
	}
	seconds := base * multiplier
	if seconds < 1 {
		return 1
	}
	return seconds
}

func (m *model) currentLoggerError() error {
	if m.logErr != nil {
		return m.logErr
	}
	if m.logger != nil {
		return m.logger.LastError()
	}
	return nil
}

func (m *model) syncLoggerErrorToLogsView() {
	if m.logsView == nil {
		return
	}
	m.logsView.SetLoggerError(m.currentLoggerError())
}

func (m *model) updateActiveView(msg tea.Msg) tea.Cmd {
	switch m.screen {
	case screenConfig:
		if m.configView != nil {
			return m.configView.Update(msg, m.keys)
		}
	case screenLogs:
		if m.logsView != nil {
			return m.logsView.Update(msg)
		}
	case screenPin:
		if m.pinView != nil {
			return m.pinView.Update(msg, m.keys, m.loopMode())
		}
	case screenBuildSpecs:
		if m.specsView != nil {
			return m.specsView.Update(msg, m.keys)
		}
	case screenRunLoop:
		if m.loopView != nil {
			return m.loopView.Update(msg, m.keys)
		}
	}
	return nil
}

func (m *model) setLogger(cfg config.Config) {
	if m.logger != nil {
		_ = m.logger.Close()
	}
	logger, err := newTUILogger(cfg)
	if err != nil {
		m.logErr = err
		m.logger = nil
	} else {
		m.logErr = nil
		m.logger = logger
	}

	logPath, pathErr := resolveLogPath(cfg)
	if pathErr != nil {
		logPath = ""
	}
	if m.logger != nil {
		logPath = m.logger.Path()
	}
	if m.logsView != nil {
		m.logsView.SetLogPath(logPath)
		m.logsView.SetLoggerError(m.currentLoggerError())
	}
	if m.loopView != nil {
		m.loopView.logger = m.logger
	}
	if m.pinView != nil {
		m.pinView.logger = m.logger
	}
	if m.specsView != nil {
		m.specsView.logger = m.logger
	}
	if m.repoStatusSampler != nil {
		m.repoStatusSampler.SetLogger(m.logger)
	}
}

func (m *model) closeLogger() {
	if m.logger != nil {
		_ = m.logger.Close()
	}
	m.logger = nil
	if m.repoStatusSampler != nil {
		m.repoStatusSampler.SetLogger(nil)
	}
}

func (m *model) Shutdown(reason string) {
	if m == nil || m.shuttingDown {
		return
	}
	m.shuttingDown = true

	if m.runCancel != nil {
		m.runCancel()
	}
	if m.loopView != nil {
		m.loopView.stop()
	}
	if m.specsView != nil {
		m.specsView.cancelBuild()
	}
	if reason != "" {
		m.logInfo("tui.shutdown", map[string]any{"reason": reason})
	}
	m.closeLogger()
}

func (m *model) ShutdownWait(timeout time.Duration) {
	if m == nil || timeout <= 0 {
		return
	}
	deadline := time.Now().Add(timeout)
	waitFor := func(ch <-chan struct{}) {
		if ch == nil {
			return
		}
		remaining := time.Until(deadline)
		if remaining <= 0 {
			return
		}
		select {
		case <-ch:
		case <-time.After(remaining):
		}
	}
	if m.loopView != nil {
		waitFor(m.loopView.runDone)
	}
	if m.specsView != nil {
		waitFor(m.specsView.buildDone)
	}
}

func (m *model) logDebug(message string, fields map[string]any) {
	if m.logger != nil {
		m.logger.Debug(message, fields)
	}
}

func (m *model) logInfo(message string, fields map[string]any) {
	if m.logger != nil {
		m.logger.Info(message, fields)
	}
}

func (m *model) logError(message string, err error) {
	if m.logger != nil && err != nil {
		m.logger.Error(message, map[string]any{"error": err.Error()})
	}
}

func (m *model) helpKeyMap() help.KeyMap {
	if m.searchActive {
		return searchKeyMap{keys: m.keys}
	}
	screenKeys := m.screenKeyMap()
	if m.isTyping() {
		return mergedKeyMap{global: typingGlobalKeyMap{keys: m.keys}, screen: screenKeys}
	}
	return mergedKeyMap{global: globalKeyMap{keys: m.keys}, screen: screenKeys}
}

func (m *model) screenKeyMap() help.KeyMap {
	switch m.screen {
	case screenDashboard:
		return dashboardKeyMap{keys: m.keys}
	case screenConfig:
		return configKeyMap{keys: m.keys}
	case screenPin:
		return pinKeyMap{keys: m.keys}
	case screenBuildSpecs:
		running := false
		if m.specsView != nil {
			running = m.specsView.running
		}
		return specsKeyMap{keys: m.keys, running: running}
	case screenRunLoop:
		supportsEffort := false
		if m.loopView != nil {
			supportsEffort = runnerargs.SupportsReasoningEffort(m.loopView.overrides.Runner)
		}
		return loopKeyMap{keys: m.keys, mode: m.loopMode(), supportsEffort: supportsEffort}
	case screenLogs:
		return logsKeyMap{keys: m.keys}
	default:
		return emptyKeyMap{}
	}
}

func (m model) focusedPanelEffective() focusedPanel {
	if m.navCollapsed {
		return focusedPanelContent
	}
	if m.searchActive {
		return focusedPanelNav
	}
	if m.navFocused {
		return focusedPanelNav
	}
	return focusedPanelContent
}

func (m model) navPanelFocusedEffective() bool {
	return m.focusedPanelEffective() == focusedPanelNav
}

func (m *model) applyFocus() {
	if m.navCollapsed {
		m.navFocused = false
	}
	navFocused := m.navPanelFocusedEffective()
	if m.pinView != nil {
		if navFocused || m.screen != screenPin {
			m.pinView.Blur()
		} else {
			m.pinView.Focus()
		}
	}
	if m.specsView != nil {
		if navFocused || m.screen != screenBuildSpecs {
			m.specsView.Blur()
		} else {
			m.specsView.Focus()
		}
	}
	if m.loopView != nil {
		if navFocused || m.screen != screenRunLoop {
			m.loopView.Blur()
		} else {
			m.loopView.Focus()
		}
	}
}

func (m *model) applyLoopRunMode(running bool) {
	if running {
		if !m.navCollapsed {
			m.loopNavWasCollapsed = m.navCollapsed
			m.navCollapsed = true
			m.navFocused = false
			m.loopAutoCollapsed = true
			m.applyFocus()
			m.relayout()
		}
		return
	}
	if m.loopAutoCollapsed {
		m.navCollapsed = m.loopNavWasCollapsed
		m.loopAutoCollapsed = false
		if m.navCollapsed {
			m.navFocused = false
		}
		m.applyFocus()
		m.relayout()
	}
}

func (m *model) loopMode() loopMode {
	if m.loopView == nil {
		return loopIdle
	}
	return m.loopView.mode
}

func (m *model) beginSearch() {
	if m.searchActive {
		return
	}
	m.searchNavCollapsed = m.navCollapsed
	if m.navCollapsed {
		m.navCollapsed = false
	}
	m.searchActive = true
	m.searchInput.SetValue("")
	m.searchInput.Focus()
	m.priorNavSelected = m.nav.Index()
	m.nav.SetItems(m.navBaseItems)
	m.restoreNavSelection(m.priorNavSelected)
	m.applyFocus()
	m.relayout()
}

func isSearchSelectionKey(msg tea.KeyMsg) bool {
	switch msg.Type {
	case tea.KeyUp, tea.KeyDown, tea.KeyPgUp, tea.KeyPgDown, tea.KeyHome, tea.KeyEnd:
		return true
	default:
		return false
	}
}

func (m *model) routeNavSelectionKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyHome:
		m.nav.Select(0)
		return nil
	case tea.KeyEnd:
		items := m.nav.Items()
		if len(items) > 0 {
			m.nav.Select(len(items) - 1)
		}
		return nil
	default:
		updated, cmd := m.nav.Update(msg)
		m.nav = updated
		return cmd
	}
}

func (m *model) updateSearch(msg tea.KeyMsg) tea.Cmd {
	if isSearchSelectionKey(msg) {
		return m.routeNavSelectionKey(msg)
	}
	prevValue := m.searchInput.Value()
	updated, cmd := m.searchInput.Update(msg)
	m.searchInput = updated
	if prevValue != m.searchInput.Value() {
		m.rebuildSearchResults(m.searchInput.Value())
	}

	if key.Matches(msg, m.keys.Select) || msg.Type == tea.KeyEnter {
		return m.acceptSearch()
	}
	if key.Matches(msg, m.keys.SearchCancel) || msg.Type == tea.KeyEsc {
		m.cancelSearch()
		return nil
	}
	return cmd
}

func (m *model) rebuildSearchResults(term string) {
	parts := searchParts(term)
	prevKey := navKeyOf(m.nav.SelectedItem())
	if len(parts) == 0 {
		m.nav.SetItems(m.navBaseItems)
		if !m.selectNavItemByKey(prevKey) {
			m.restoreNavSelection(m.priorNavSelected)
		}
		return
	}

	results := make([]list.Item, 0)
	if m.pinView != nil {
		for _, entry := range m.pinView.SearchEntries() {
			if matchesPinEntryForSearch(entry, parts) {
				results = append(results, pinResultItem{
					section: entry.Section,
					id:      entry.ID,
					header:  entry.Header,
				})
			}
		}
	}
	for _, item := range navigationItems() {
		if matchesNavItem(item, parts) {
			results = append(results, item)
		}
	}

	m.nav.SetItems(results)
	if !m.selectNavItemByKey(prevKey) && len(results) > 0 {
		m.nav.Select(0)
	}
}

func (m *model) acceptSearch() tea.Cmd {
	if !m.searchActive {
		return nil
	}
	selected := m.nav.SelectedItem()
	m.searchActive = false
	m.searchInput.Blur()
	m.nav.SetItems(m.navBaseItems)
	if m.searchNavCollapsed {
		m.navCollapsed = true
		m.navFocused = false
	}
	switch item := selected.(type) {
	case navItem:
		cmds := m.switchScreen(item.screen, true)
		return tea.Batch(cmds...)
	case pinResultItem:
		cmds := m.switchScreen(screenPin, true)
		if m.pinView != nil {
			m.pinView.SelectItem(item.section, item.id)
		}
		return tea.Batch(cmds...)
	}
	m.restoreNavSelection(m.priorNavSelected)
	m.applyFocus()
	m.relayout()
	return nil
}

func (m *model) cancelSearch() {
	if !m.searchActive {
		return
	}
	m.searchActive = false
	m.searchInput.Blur()
	m.nav.SetItems(m.navBaseItems)
	m.restoreNavSelection(m.priorNavSelected)
	if m.searchNavCollapsed {
		m.navCollapsed = true
		m.navFocused = false
	}
	m.applyFocus()
	m.relayout()
}

func (m *model) switchScreen(next screen, focusContent bool) []tea.Cmd {
	prev := m.screen
	m.screen = next
	m.selectNavItem(next)
	if focusContent {
		m.navFocused = false
	}
	m.applyFocus()
	m.relayout()
	cmds := m.refreshScreen(next, false)
	if prev != next {
		m.logInfo("screen.change", map[string]any{"from": screenName(prev), "to": screenName(next)})
	}
	return cmds
}

func (m *model) selectNavItem(target screen) {
	index := navIndexForScreen(target)
	if index >= 0 {
		m.nav.Select(index)
	}
}

func navIndexForScreen(target screen) int {
	items := navigationItems()
	for i, item := range items {
		if item.screen == target {
			return i
		}
	}
	return -1
}

func navItemsAsList() []list.Item {
	items := navigationItems()
	listItems := make([]list.Item, 0, len(items))
	for _, item := range items {
		listItems = append(listItems, item)
	}
	return listItems
}

func (m *model) restoreNavSelection(index int) {
	items := m.nav.Items()
	if len(items) == 0 {
		return
	}
	if index < 0 || index >= len(items) {
		m.nav.Select(0)
		return
	}
	m.nav.Select(index)
}

func (m *model) selectNavItemByKey(key string) bool {
	if key == "" {
		return false
	}
	items := m.nav.Items()
	for idx, item := range items {
		if navKeyOf(item) == key {
			m.nav.Select(idx)
			return true
		}
	}
	return false
}
