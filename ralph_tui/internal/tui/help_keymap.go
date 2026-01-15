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
	return []key.Binding{g.keys.Quit, g.keys.ToggleNav, g.keys.Focus, g.keys.Help, g.keys.RefreshNow, g.keys.Select}
}

func (g globalKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{g.keys.Quit, g.keys.ToggleNav, g.keys.Focus, g.keys.Help, g.keys.RefreshNow, g.keys.Select}}
}

type pinKeyMap struct {
	keys keyMap
}

func (p pinKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{p.keys.ValidatePin, p.keys.MoveChecked, p.keys.BlockItem, p.keys.ToggleChecked}
}

func (p pinKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{p.keys.ValidatePin, p.keys.MoveChecked, p.keys.BlockItem, p.keys.ToggleChecked}, {p.keys.TogglePane}}
}

type specsKeyMap struct {
	keys keyMap
}

func (s specsKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{s.keys.EditSpecsSettings, s.keys.ToggleInteractive, s.keys.ToggleInnovate, s.keys.ToggleScoutWorkflow, s.keys.RunSpecs, s.keys.StopSpecs}
}

func (s specsKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{s.keys.EditSpecsSettings, s.keys.ToggleInteractive, s.keys.ToggleInnovate, s.keys.ToggleAutofill, s.keys.ToggleScoutWorkflow, s.keys.EditUserFocus}, {s.keys.RunSpecs, s.keys.StopSpecs}}
}

type loopKeyMap struct {
	keys keyMap
}

func (l loopKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{l.keys.RunLoopOnce, l.keys.RunLoopContinuous, l.keys.StopLoop, l.keys.EditLoopConfig}
}

func (l loopKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{l.keys.RunLoopOnce, l.keys.RunLoopContinuous, l.keys.StopLoop, l.keys.EditLoopConfig}}
}

type dashboardKeyMap struct {
	keys keyMap
}

func (d dashboardKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{d.keys.DashboardRunLoopOnce, d.keys.DashboardBuildSpecs}
}

func (d dashboardKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{d.keys.DashboardRunLoopOnce, d.keys.DashboardBuildSpecs}}
}

type configKeyMap struct {
	keys keyMap
}

func (c configKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{c.keys.SaveGlobal, c.keys.SaveRepo, c.keys.Discard}
}

func (c configKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{c.keys.SaveGlobal, c.keys.SaveRepo, c.keys.Discard}}
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
