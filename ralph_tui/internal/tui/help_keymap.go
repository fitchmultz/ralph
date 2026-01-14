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
	return []key.Binding{g.keys.Quit, g.keys.Focus, g.keys.Help, g.keys.Select}
}

func (g globalKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{g.keys.Quit, g.keys.Focus, g.keys.Help, g.keys.Select}}
}

type pinKeyMap struct {
	keys keyMap
}

func (p pinKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{p.keys.ValidatePin, p.keys.MoveChecked, p.keys.BlockItem}
}

func (p pinKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{p.keys.ValidatePin, p.keys.MoveChecked, p.keys.BlockItem}}
}

type specsKeyMap struct {
	keys keyMap
}

func (s specsKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{s.keys.ToggleInteractive, s.keys.ToggleInnovate, s.keys.ToggleAutofill, s.keys.RunSpecs}
}

func (s specsKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{s.keys.ToggleInteractive, s.keys.ToggleInnovate, s.keys.ToggleAutofill, s.keys.RunSpecs}}
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

type configKeyMap struct {
	keys keyMap
}

func (c configKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{c.keys.SaveGlobal, c.keys.SaveRepo, c.keys.Discard}
}

func (c configKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{c.keys.SaveGlobal, c.keys.SaveRepo, c.keys.Discard}}
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
