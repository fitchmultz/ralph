// Package tui provides contextual help keymaps for the Ralph TUI.
package tui

import (
	"github.com/charmbracelet/bubbles/help"
	"github.com/charmbracelet/bubbles/key"
)

type globalKeyMap struct {
	keys keyMap
}

func (g globalKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{g.keys.Quit, g.keys.ToggleNav, g.keys.Focus, g.keys.Help, g.keys.RefreshNow, g.keys.Search, g.keys.Select}
}

func (g globalKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{g.keys.Quit, g.keys.ToggleNav, g.keys.Focus, g.keys.Help, g.keys.RefreshNow, g.keys.Search, g.keys.Select}}
}

type searchKeyMap struct {
	keys            keyMap
	canToggleTarget bool
}

func (s searchKeyMap) ShortHelp() []key.Binding {
	bindings := []key.Binding{s.keys.Quit, s.keys.SearchCancel, s.keys.Select}
	if s.canToggleTarget {
		bindings = append(bindings, s.keys.SearchTargetToggle)
	}
	return bindings
}

func (s searchKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{s.ShortHelp()}
}

type pinKeyMap struct {
	keys keyMap
}

func (p pinKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{
		p.keys.TogglePinSection,
		p.keys.ValidatePin,
		p.keys.MoveChecked,
		p.keys.BlockItem,
		p.keys.ToggleChecked,
	}
}

func (p pinKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{
		{p.keys.TogglePinSection, p.keys.ValidatePin, p.keys.MoveChecked, p.keys.BlockItem, p.keys.ToggleChecked},
		{p.keys.TogglePane},
	}
}

type specsKeyMap struct {
	keys    keyMap
	running bool
}

func (s specsKeyMap) ShortHelp() []key.Binding {
	return specsKeyHintBindings(s.keys, s.running)
}

func (s specsKeyMap) FullHelp() [][]key.Binding {
	return specsKeyHelpGroups(s.keys, s.running)
}

type loopKeyMap struct {
	keys           keyMap
	mode           loopMode
	supportsEffort bool
}

func (l loopKeyMap) ShortHelp() []key.Binding {
	return loopKeyHintBindings(l.keys, l.mode, l.supportsEffort)
}

func (l loopKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{l.ShortHelp()}
}

type dashboardKeyMap struct {
	keys keyMap
}

func (d dashboardKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{d.keys.DashboardRunLoopOnce, d.keys.DashboardFixupBlocked, d.keys.DashboardBuildSpecs}
}

func (d dashboardKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{d.keys.DashboardRunLoopOnce, d.keys.DashboardFixupBlocked, d.keys.DashboardBuildSpecs}}
}

type configKeyMap struct {
	keys keyMap
}

func (c configKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{c.keys.SaveGlobal, c.keys.SaveRepo, c.keys.Discard, c.keys.ResetField, c.keys.ResetLayer}
}

func (c configKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{c.keys.SaveGlobal, c.keys.SaveRepo, c.keys.Discard, c.keys.ResetField, c.keys.ResetLayer}}
}

type logsKeyMap struct {
	keys keyMap
}

func (l logsKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{l.keys.ToggleLogsFormat}
}

func (l logsKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{l.keys.ToggleLogsFormat}}
}

type emptyKeyMap struct{}

func (emptyKeyMap) ShortHelp() []key.Binding  { return nil }
func (emptyKeyMap) FullHelp() [][]key.Binding { return nil }

type mergedKeyMap struct {
	global help.KeyMap
	screen help.KeyMap
}

func (m mergedKeyMap) ShortHelp() []key.Binding {
	short := make([]key.Binding, 0)
	short = append(short, shortHelp(m.global)...)
	short = append(short, shortHelp(m.screen)...)
	return short
}

func (m mergedKeyMap) FullHelp() [][]key.Binding {
	full := make([][]key.Binding, 0)
	full = append(full, fullHelp(m.global)...)
	full = append(full, fullHelp(m.screen)...)
	return full
}

func shortHelp(km help.KeyMap) []key.Binding {
	if km == nil {
		return nil
	}
	return km.ShortHelp()
}

func fullHelp(km help.KeyMap) [][]key.Binding {
	if km == nil {
		return nil
	}
	return km.FullHelp()
}
