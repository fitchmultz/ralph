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
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
)

// Start launches the TUI and blocks until it exits.
func Start(cfg config.Config, locations paths.Locations) error {
	program := tea.NewProgram(newModel(cfg, locations), tea.WithAltScreen())
	_, err := program.Run()
	return err
}

type model struct {
	nav        list.Model
	screen     screen
	help       help.Model
	keys       keyMap
	navFocused bool
	cfg        config.Config
	configView *configEditor
	pinView    *pinView
	specsView  *specsView
	loopView   *loopView
	width      int
	height     int
	layout     layoutSpec
	initErr    error
	locations  paths.Locations
}

func newModel(cfg config.Config, locations paths.Locations) model {
	items := make([]list.Item, 0)
	for _, item := range navigationItems() {
		items = append(items, item)
	}

	l := list.New(items, list.NewDefaultDelegate(), 24, 16)
	l.Title = "Ralph"
	l.SetShowFilter(false)
	l.SetShowStatusBar(false)
	l.SetFilteringEnabled(false)
	l.SetShowHelp(false)

	var err error

	configView, configErr := newConfigEditor(locations)
	if err == nil {
		err = configErr
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

	m := model{
		nav:        l,
		screen:     screenDashboard,
		help:       help.New(),
		keys:       newKeyMap(),
		navFocused: true,
		cfg:        cfg,
		configView: configView,
		pinView:    pinView,
		specsView:  specsView,
		loopView:   loopView,
		initErr:    err,
		locations:  locations,
	}
	m.layout = computeLayoutWithBody(0, 0)
	m.resizeViews(0, 0)
	m.applyFocus()
	return m
}

func (m model) Init() tea.Cmd {
	cmds := []tea.Cmd{refreshCmd(m.cfg.UI.RefreshSeconds)}
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
		m.width = msg.Width
		m.height = msg.Height
		m.relayout()
		cmds = append(cmds, m.postResizeCmds()...)
		handled = true
	case refreshMsg:
		cmds = append(cmds, m.refreshViews()...)
		cmds = append(cmds, refreshCmd(m.cfg.UI.RefreshSeconds))
		handled = true
	case configReloadMsg:
		if msg.err != nil {
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
		cmds = append(cmds, refreshCmd(m.cfg.UI.RefreshSeconds))
		handled = true
	case tea.KeyMsg:
		if key.Matches(msg, m.keys.Quit) {
			return m, tea.Quit
		}
		if key.Matches(msg, m.keys.Help) {
			m.help.ShowAll = !m.help.ShowAll
			m.relayout()
			return m, nil
		}
		if key.Matches(msg, m.keys.Focus) {
			m.navFocused = !m.navFocused
			m.applyFocus()
			m.relayout()
			return m, nil
		}
		if m.screen == screenConfig {
			if key.Matches(msg, m.keys.SaveGlobal) && m.configView != nil {
				m.configView.SaveGlobal()
				return m, m.reloadConfigCmd()
			}
			if key.Matches(msg, m.keys.SaveRepo) && m.configView != nil {
				m.configView.SaveRepo()
				return m, m.reloadConfigCmd()
			}
			if key.Matches(msg, m.keys.Discard) && m.configView != nil {
				m.configView.DiscardSession()
				return m, m.reloadConfigCmd()
			}
		}
		if m.navFocused {
			updated, cmd := m.nav.Update(msg)
			m.nav = updated
			cmds = append(cmds, cmd)

			if key.Matches(msg, m.keys.Select) {
				if item, ok := m.nav.SelectedItem().(navItem); ok {
					m.screen = item.screen
					m.applyFocus()
					m.relayout()
				}
			}
		} else {
			cmds = append(cmds, m.updateActiveView(msg))
		}

		return m, tea.Batch(cmds...)
	}

	if !handled {
		cmds = append(cmds, m.updateActiveView(msg))
	}

	return m, tea.Batch(cmds...)
}

func (m model) View() string {
	if m.initErr != nil {
		return fmt.Sprintf("Error: %v\n", m.initErr)
	}

	navView := strings.TrimRight(m.nav.View(), "\n")
	contentView := strings.TrimRight(m.contentView(), "\n")

	navStyle, contentStyle := m.panelStyles()
	navFrameW, navFrameH := navStyle.GetFrameSize()
	contentFrameW, contentFrameH := contentStyle.GetFrameSize()

	navInnerW := max(0, m.layout.navWidth-navFrameW)
	contentInnerW := max(0, m.layout.contentWidth-contentFrameW)
	navInnerH := max(0, m.layout.bodyHeight-navFrameH)
	contentInnerH := max(0, m.layout.bodyHeight-contentFrameH)

	navStyle = navStyle.Width(navInnerW).Height(navInnerH)
	contentStyle = contentStyle.Width(contentInnerW).Height(contentInnerH)

	left := navStyle.Render(navView)
	right := contentStyle.Render(contentView)
	body := lipgloss.JoinHorizontal(lipgloss.Top, left, right)
	body = strings.TrimRight(body, "\n")
	body = clampToSize(body, m.width, m.layout.bodyHeight)

	m.help.Width = m.width
	footer := strings.TrimRight(m.help.View(m.helpKeyMap()), "\n")
	footer = clampToSize(footer, m.width, 0)
	rendered := body
	if lipgloss.Height(footer) > 0 {
		rendered = body + strings.Repeat("\n", footerGapBlankLines+1) + footer
	}
	rendered = clampToSize(rendered, m.width, m.height)
	return withFinalNewline(rendered)
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func (m model) contentView() string {
	switch m.screen {
	case screenDashboard:
		return "Dashboard\n\nSummary panels will land here."
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
		return "Logs\n\nLogs viewer will land here."
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
}

const (
	defaultNavWidth     = 26
	defaultContentWidth = 80
	minNavWidth         = 20
	minContentWidth     = 30
	footerGapBlankLines = 1
)

func computeLayoutWithBody(width int, bodyHeight int) layoutSpec {
	if bodyHeight < 0 {
		bodyHeight = 0
	}
	if width <= 0 {
		return layoutSpec{
			navWidth:     defaultNavWidth,
			contentWidth: defaultContentWidth,
			bodyHeight:   bodyHeight,
		}
	}

	navWidth := defaultNavWidth
	maxNav := width / 3
	if maxNav < minNavWidth {
		maxNav = minNavWidth
	}
	if navWidth > maxNav {
		navWidth = maxNav
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
		if navWidth > width {
			navWidth = width
		}
		if navWidth < 0 {
			navWidth = 0
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

func (m model) panelStyles() (lipgloss.Style, lipgloss.Style) {
	border := lipgloss.RoundedBorder()
	focused := lipgloss.AdaptiveColor{Light: "63", Dark: "75"}
	unfocused := lipgloss.AdaptiveColor{Light: "245", Dark: "238"}

	navStyle := lipgloss.NewStyle().
		Padding(1, 1, 0, 1).
		Border(border)

	contentStyle := lipgloss.NewStyle().
		Padding(1, 1, 0, 1).
		Border(border)

	if m.navFocused {
		navStyle = navStyle.BorderForeground(focused)
		contentStyle = contentStyle.BorderForeground(unfocused)
	} else {
		navStyle = navStyle.BorderForeground(unfocused)
		contentStyle = contentStyle.BorderForeground(focused)
	}

	return navStyle, contentStyle
}

func (m *model) relayout() {
	if m.width <= 0 || m.height <= 0 {
		return
	}

	m.help.Width = m.width
	footer := m.help.View(m.helpKeyMap())
	footerH := lipgloss.Height(footer)

	footerGap := 0
	if footerH > 0 {
		footerGap = footerGapBlankLines + 1
	}
	bodyH := m.height - footerH - footerGap
	if bodyH < 0 {
		bodyH = 0
	}

	m.layout = computeLayoutWithBody(m.width, bodyH)

	navStyle, contentStyle := m.panelStyles()
	navFrameW, navFrameH := navStyle.GetFrameSize()
	contentFrameW, contentFrameH := contentStyle.GetFrameSize()

	navInnerW := max(0, m.layout.navWidth-navFrameW)
	contentInnerW := max(0, m.layout.contentWidth-contentFrameW)
	navInnerH := max(0, m.layout.bodyHeight-navFrameH)
	contentInnerH := max(0, m.layout.bodyHeight-contentFrameH)

	m.nav.SetSize(navInnerW, navInnerH)
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

type refreshMsg struct{}

type configReloadMsg struct {
	cfg config.Config
	err error
}

func refreshCmd(seconds int) tea.Cmd {
	if seconds <= 0 {
		return nil
	}
	return tea.Tick(time.Duration(seconds)*time.Second, func(time.Time) tea.Msg {
		return refreshMsg{}
	})
}

func (m model) reloadConfigCmd() tea.Cmd {
	locations := m.locations
	return func() tea.Msg {
		cfg, err := config.LoadFromLocations(config.LoadOptions{Locations: locations})
		return configReloadMsg{cfg: cfg, err: err}
	}
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
	return cmds
}

func (m *model) updateActiveView(msg tea.Msg) tea.Cmd {
	switch m.screen {
	case screenConfig:
		if m.configView != nil {
			return m.configView.Update(msg)
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

func (m *model) helpKeyMap() help.KeyMap {
	global := globalKeyMap{keys: m.keys}
	screenKeys := m.screenKeyMap()
	return mergedKeyMap{global: global, screen: screenKeys}
}

func (m *model) screenKeyMap() help.KeyMap {
	switch m.screen {
	case screenConfig:
		return configKeyMap{keys: m.keys}
	case screenPin:
		return pinKeyMap{keys: m.keys}
	case screenBuildSpecs:
		return specsKeyMap{keys: m.keys}
	case screenRunLoop:
		return loopKeyMap{keys: m.keys}
	default:
		return emptyKeyMap{}
	}
}

func (m *model) applyFocus() {
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
