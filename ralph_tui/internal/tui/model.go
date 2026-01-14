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
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/config"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/paths"
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
	m.layout = computeLayout(0, 0)
	m.resizeViews()
	m.applyFocus()
	return m
}

func (m model) Init() tea.Cmd {
	return refreshCmd(m.cfg.UI.RefreshSeconds)
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd
	handled := false

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.layout = computeLayout(m.width, m.height)
		m.help.Width = m.width
		m.nav.SetSize(max(10, m.layout.navWidth-4), max(6, m.layout.contentHeight))
		m.resizeViews()
		handled = true
	case refreshMsg:
		m.refreshViews()
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
			return m, nil
		}
		if key.Matches(msg, m.keys.Focus) {
			m.navFocused = !m.navFocused
			m.applyFocus()
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

	navView := m.nav.View()
	contentView := m.contentView()

	navWidth := m.layout.navWidth
	if navWidth == 0 {
		navWidth = defaultNavWidth
	}
	contentWidth := m.layout.contentWidth
	if contentWidth == 0 {
		contentWidth = defaultContentWidth
	}

	navBorder := lipgloss.HiddenBorder()
	contentBorder := lipgloss.HiddenBorder()
	if m.navFocused {
		navBorder = lipgloss.RoundedBorder()
	} else {
		contentBorder = lipgloss.RoundedBorder()
	}
	navStyle := lipgloss.NewStyle().
		Width(max(10, navWidth-4)).
		Padding(1, 1, 0, 1).
		Border(navBorder)
	contentStyle := lipgloss.NewStyle().
		Width(max(10, contentWidth-4)).
		Padding(1, 1, 0, 1).
		Border(contentBorder)

	left := navStyle.Render(navView)
	right := contentStyle.Render(contentView)
	body := lipgloss.JoinHorizontal(lipgloss.Top, left, right)

	footer := m.help.View(m.helpKeyMap())
	return strings.TrimSpace(body+"\n\n"+footer) + "\n"
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
	navWidth      int
	contentWidth  int
	contentHeight int
}

const (
	defaultNavWidth     = 26
	defaultContentWidth = 80
	minNavWidth         = 20
	minContentWidth     = 30
)

func computeLayout(width int, height int) layoutSpec {
	navWidth := defaultNavWidth
	if width > 0 {
		maxNav := width / 3
		if maxNav < minNavWidth {
			maxNav = minNavWidth
		}
		if navWidth > maxNav {
			navWidth = maxNav
		}
	}

	contentWidth := width - navWidth - 4
	if contentWidth < minContentWidth {
		contentWidth = minContentWidth
	}
	if width == 0 {
		contentWidth = defaultContentWidth
	}

	contentHeight := height - 6
	if contentHeight < 5 {
		contentHeight = 5
	}

	return layoutSpec{
		navWidth:      navWidth,
		contentWidth:  contentWidth,
		contentHeight: contentHeight,
	}
}

func (m *model) resizeViews() {
	contentWidth := max(10, m.layout.contentWidth-4)
	if m.configView != nil {
		m.configView.Resize(contentWidth, m.layout.contentHeight)
	}
	if m.pinView != nil {
		m.pinView.Resize(contentWidth, m.layout.contentHeight)
	}
	if m.specsView != nil {
		m.specsView.Resize(contentWidth, m.layout.contentHeight)
	}
	if m.loopView != nil {
		m.loopView.Resize(contentWidth, m.layout.contentHeight)
	}
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

func (m *model) refreshViews() {
	if m.pinView != nil {
		m.pinView.RefreshIfNeeded()
	}
	if m.specsView != nil {
		m.specsView.RefreshIfNeeded()
	}
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
