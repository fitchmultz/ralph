// Package tui provides the Bubble Tea model for the Ralph application shell.
// Entrypoint: Start.
package tui

import (
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
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
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
		m.closeLogger()
	case *model:
		m.closeLogger()
	}
	return err
}

type model struct {
	nav                list.Model
	screen             screen
	help               help.Model
	keys               keyMap
	searchInput        textinput.Model
	searchActive       bool
	searchTarget       searchTarget
	searchErr          string
	priorNavSelected   int
	searchNavCollapsed bool
	navFocused         bool
	navCollapsed       bool
	cfg                config.Config
	configView         *configEditor
	pinView            *pinView
	specsView          *specsView
	loopView           *loopView
	logsView           *logsView
	logger             *tuiLogger
	logErr             error
	cliOverrides       config.PartialConfig
	sessionOverrides   config.PartialConfig
	refreshGen         int
	width              int
	height             int
	layout             layoutSpec
	initErr            error
	locations          paths.Locations
}

type searchTarget int

const (
	searchTargetNav searchTarget = iota
	searchTargetPin
)

func newModel(cfg config.Config, locations paths.Locations, opts StartOptions) model {
	items := make([]list.Item, 0)
	for _, item := range navigationItems() {
		items = append(items, item)
	}

	l := list.New(items, list.NewDefaultDelegate(), 24, 16)
	l.Title = "Ralph"
	l.SetShowFilter(false)
	l.SetShowStatusBar(false)
	l.SetFilteringEnabled(true)
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
	if err == nil {
		missing := pin.MissingFiles(pinFiles)
		if len(missing) > 0 {
			err = fmt.Errorf(
				"Ralph pin files missing:\n- %s\n\nRun `ralph init` to create defaults.",
				strings.Join(missing, "\n- "),
			)
		} else if pinErr := pin.ValidatePin(pinFiles); pinErr != nil {
			err = fmt.Errorf(
				"Ralph pin files are invalid: %v\n\nRun `ralph pin validate` for details or `ralph init --force` to reset defaults.",
				pinErr,
			)
		}
	}

	pinView, pinErr := newPinView(cfg, locations)
	if err == nil {
		err = pinErr
	}

	specsView, specsErr := newSpecsView(cfg, locations)
	if err == nil {
		err = specsErr
	}

	loopView := newLoopView(cfg, locations)
	logsView := newLogsView("")

	m := model{
		nav:              l,
		screen:           screenDashboard,
		help:             help.New(),
		keys:             newKeyMap(),
		searchInput:      searchInput,
		priorNavSelected: l.Index(),
		navFocused:       true,
		navCollapsed:     false,
		cfg:              cfg,
		configView:       configView,
		pinView:          pinView,
		specsView:        specsView,
		loopView:         loopView,
		logsView:         logsView,
		cliOverrides:     opts.CLIOverrides,
		sessionOverrides: opts.SessionOverrides,
		refreshGen:       1,
		initErr:          err,
		locations:        locations,
	}
	m.setLogger(cfg)
	if m.logsView != nil {
		var loopLines []string
		if m.loopView != nil {
			loopLines = m.loopView.LogLines()
		}
		var specsLines []string
		if m.specsView != nil {
			specsLines = m.specsView.RunLogLines()
		}
		m.logsView.Refresh(loopLines, specsLines)
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
	cmds := []tea.Cmd{refreshCmd(m.cfg.UI.RefreshSeconds, m.refreshGen)}
	if m.pinView != nil {
		if cmd := m.pinView.reloadAsync(true); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	if m.specsView != nil {
		if cmd := m.specsView.RefreshPreviewCmd(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	return tea.Batch(cmds...)
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd
	handled := false

	switch msg := msg.(type) {
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
		cmds = append(cmds, refreshCmd(m.cfg.UI.RefreshSeconds, m.refreshGen))
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
		cmds = append(cmds, refreshCmd(m.cfg.UI.RefreshSeconds, m.refreshGen))
		m.logInfo("config.reload", map[string]any{
			"refresh_seconds": m.cfg.UI.RefreshSeconds,
			"log_level":       m.cfg.Logging.Level,
			"log_file":        m.cfg.Logging.File,
			"refresh_gen":     m.refreshGen,
		})
		handled = true
	case tea.KeyMsg:
		keyFields := keyEventSummary(msg)
		keyFields["screen"] = screenName(m.screen)
		keyFields["nav_focused"] = m.navFocused
		keyFields["nav_collapsed"] = m.navCollapsed
		m.logDebug("key.event", keyFields)
		if key.Matches(msg, m.keys.Quit) {
			m.logInfo("tui.quit", map[string]any{"screen": screenName(m.screen)})
			m.closeLogger()
			return m, tea.Quit
		}
		if key.Matches(msg, m.keys.ToggleNav) {
			m.navCollapsed = !m.navCollapsed
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
		if m.screen == screenDashboard {
			switch {
			case key.Matches(msg, m.keys.DashboardRunLoopOnce):
				if m.loopView == nil {
					return m, nil
				}
				return m, m.loopView.StartOnce()
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
		if key.Matches(msg, m.keys.ToggleLogsFormat) && m.screen == screenLogs && m.logsView != nil {
			m.logsView.ToggleFormat()
			return m, nil
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
			if m.shouldBypassFocusToggle(msg) {
				break
			}
			m.navFocused = !m.navFocused
			m.applyFocus()
			m.relayout()
			return m, nil
		}
		if m.screen == screenConfig {
			if key.Matches(msg, m.keys.SaveGlobal) && m.configView != nil {
				m.configView.SaveGlobal()
				m.syncSessionOverridesFromEditor()
				return m, m.reloadConfigCmd()
			}
			if key.Matches(msg, m.keys.SaveRepo) && m.configView != nil {
				m.configView.SaveRepo()
				m.syncSessionOverridesFromEditor()
				return m, m.reloadConfigCmd()
			}
			if key.Matches(msg, m.keys.Discard) && m.configView != nil {
				m.configView.DiscardSession()
				m.syncSessionOverridesFromEditor()
				return m, m.reloadConfigCmd()
			}
		}
		if m.navFocused && !m.navCollapsed {
			if m.searchActive {
				cmd := m.updateSearch(msg)
				return m, cmd
			}
			updated, cmd := m.nav.Update(msg)
			m.nav = updated
			cmds = append(cmds, cmd)

			if key.Matches(msg, m.keys.Select) {
				if item, ok := m.nav.SelectedItem().(navItem); ok {
					cmds = append(cmds, m.switchScreen(item.screen, true)...)
				}
			}
		} else {
			if m.searchActive {
				cmd := m.updateSearch(msg)
				return m, cmd
			}
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

func (m model) shouldBypassFocusToggle(msg tea.KeyMsg) bool {
	if m.navFocused {
		return false
	}
	if msg.Type != tea.KeyTab {
		return false
	}
	return m.activeViewUsesTabNavigation()
}

func (m model) activeViewUsesTabNavigation() bool {
	switch m.screen {
	case screenConfig:
		return m.configView != nil && m.configView.HandlesTabNavigation()
	case screenRunLoop:
		return m.loopView != nil && m.loopView.HandlesTabNavigation()
	case screenPin:
		return m.pinView != nil && m.pinView.HandlesTabNavigation()
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
		return m.pinView.Update(msg, m.keys), true
	case specsBuildResultMsg, specsLogBatchMsg, specsPreviewMsg:
		if m.specsView == nil {
			return nil, true
		}
		return m.specsView.Update(msg, m.keys), true
	case loopResultMsg, loopLogBatchMsg:
		if m.loopView == nil {
			return nil, true
		}
		return m.loopView.Update(msg, m.keys), true
	default:
		return nil, false
	}
}

func (m model) View() string {
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
	line := m.searchInput.View()
	if m.searchErr != "" {
		line = line + " " + m.searchErr
	}
	return line
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
		return "Help\n\nUse the left menu to navigate and Tab (or Ctrl+F) to switch focus."
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

	navStyle := panelStyleFor(navOuterW, navOuterH, m.navFocused)
	contentStyle := panelStyleFor(contentOuterW, contentOuterH, !m.navFocused)

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
	m.searchInput.Width = navInnerW
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
	if m.specsView != nil {
		if cmd := m.specsView.RefreshPreviewCmd(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	return cmds
}

type refreshMsg struct {
	gen int
}

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
		var loopLines []string
		if m.loopView != nil {
			loopLines = m.loopView.LogLines()
		}
		var specsLines []string
		if m.specsView != nil {
			specsLines = m.specsView.RunLogLines()
		}
		m.logsView.Refresh(loopLines, specsLines)
	}
}

func (m *model) refreshViews() []tea.Cmd {
	cmds := make([]tea.Cmd, 0)
	if m.pinView != nil {
		if cmd := m.pinView.RefreshIfNeeded(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	if m.specsView != nil {
		if cmd := m.specsView.RefreshIfNeeded(); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	m.refreshLogsView()
	return cmds
}

func (m *model) refreshScreen(target screen, force bool) []tea.Cmd {
	cmds := make([]tea.Cmd, 0)
	switch target {
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
	var loopLines []string
	if m.loopView != nil {
		loopLines = m.loopView.LogLines()
	}
	var specsLines []string
	if m.specsView != nil {
		specsLines = m.specsView.RunLogLines()
	}
	m.logsView.Refresh(loopLines, specsLines)
}

func (m *model) updateActiveView(msg tea.Msg) tea.Cmd {
	switch m.screen {
	case screenConfig:
		if m.configView != nil {
			return m.configView.Update(msg)
		}
	case screenLogs:
		if m.logsView != nil {
			return m.logsView.Update(msg)
		}
	case screenPin:
		if m.pinView != nil {
			return m.pinView.Update(msg, m.keys)
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
		m.logsView.SetError(m.logErr)
		m.refreshLogsView()
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
}

func (m *model) closeLogger() {
	if m.logger != nil {
		_ = m.logger.Close()
	}
	m.logger = nil
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
	global := globalKeyMap{keys: m.keys}
	screenKeys := m.screenKeyMap()
	return mergedKeyMap{global: global, screen: screenKeys}
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
		return specsKeyMap{keys: m.keys}
	case screenRunLoop:
		return loopKeyMap{keys: m.keys}
	case screenLogs:
		return logsKeyMap{keys: m.keys}
	default:
		return emptyKeyMap{}
	}
}

func (m *model) applyFocus() {
	if m.navCollapsed {
		m.navFocused = false
	}
	if m.searchActive {
		m.navFocused = false
	}
	if m.pinView != nil {
		if m.navFocused || m.screen != screenPin {
			m.pinView.Blur()
		} else {
			m.pinView.Focus()
		}
	}
	if m.specsView != nil {
		if m.navFocused || m.screen != screenBuildSpecs {
			m.specsView.Blur()
		} else {
			m.specsView.Focus()
		}
	}
	if m.loopView != nil {
		if m.navFocused || m.screen != screenRunLoop {
			m.loopView.Blur()
		} else {
			m.loopView.Focus()
		}
	}
}

func (m *model) beginSearch() {
	if m.searchActive {
		return
	}
	m.searchNavCollapsed = m.navCollapsed
	if m.navCollapsed {
		m.navCollapsed = false
	}
	if m.navFocused && !m.navCollapsed {
		m.searchTarget = searchTargetNav
	} else if m.screen == screenPin {
		m.searchTarget = searchTargetPin
	} else {
		m.searchTarget = searchTargetNav
	}
	m.searchActive = true
	m.searchErr = ""
	m.searchInput.SetValue("")
	m.searchInput.Focus()
	m.priorNavSelected = m.nav.Index()
	m.updateSearchTargetState("")
	m.applyFocus()
	m.relayout()
}

func (m *model) updateSearch(msg tea.KeyMsg) tea.Cmd {
	prevValue := m.searchInput.Value()
	updated, cmd := m.searchInput.Update(msg)
	m.searchInput = updated
	if prevValue != m.searchInput.Value() {
		m.updateSearchTargetState(m.searchInput.Value())
	}

	if key.Matches(msg, m.keys.Select) || msg.Type == tea.KeyEnter {
		m.acceptSearch()
		return nil
	}
	if msg.Type == tea.KeyEsc {
		m.cancelSearch()
		return nil
	}
	if msg.Type == tea.KeyCtrlF {
		m.cancelSearch()
		m.navFocused = !m.navFocused
		m.applyFocus()
		m.relayout()
		return nil
	}
	return cmd
}

func (m *model) acceptSearch() {
	if !m.searchActive {
		return
	}
	m.searchActive = false
	m.searchInput.Blur()
	m.searchErr = ""
	if m.searchNavCollapsed {
		m.navCollapsed = true
		m.navFocused = false
	}
	if m.searchTarget == searchTargetNav {
		if item, ok := m.nav.SelectedItem().(navItem); ok {
			m.nav.ResetFilter()
			m.switchScreen(item.screen, true)
		}
	} else if m.searchTarget == searchTargetPin && m.pinView != nil {
		m.pinView.FinalizeSearch()
		m.navFocused = false
		m.applyFocus()
		m.relayout()
	}
}

func (m *model) cancelSearch() {
	if !m.searchActive {
		return
	}
	m.searchActive = false
	m.searchInput.Blur()
	m.searchErr = ""
	m.nav.ResetFilter()
	if m.pinView != nil {
		m.pinView.CancelSearch()
	}
	if m.priorNavSelected >= 0 {
		m.nav.Select(m.priorNavSelected)
	}
	if m.searchNavCollapsed {
		m.navCollapsed = true
		m.navFocused = false
	}
	m.applyFocus()
	m.relayout()
}

func (m *model) updateSearchTargetState(term string) {
	if m.searchTarget == searchTargetNav {
		m.nav.SetFilterText(term)
		m.searchErr = ""
		return
	}
	if m.searchTarget == searchTargetPin && m.pinView != nil {
		if err := m.pinView.ApplySearch(term); err != nil {
			m.searchErr = "(" + err.Error() + ")"
		} else {
			m.searchErr = ""
		}
		return
	}
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
