// Package tui defines the keybindings used by the Ralph TUI.
// Entrypoint: keyMap.
package tui

import (
	"github.com/charmbracelet/bubbles/key"
)

type keyMap struct {
	Quit              key.Binding
	Focus             key.Binding
	Help              key.Binding
	RefreshNow        key.Binding
	Select            key.Binding
	EditSpecsSettings key.Binding
	ToggleLogsFormat  key.Binding
	SaveGlobal        key.Binding
	SaveRepo          key.Binding
	Discard           key.Binding
	ValidatePin       key.Binding
	MoveChecked       key.Binding
	BlockItem         key.Binding
	ToggleChecked     key.Binding
	TogglePane        key.Binding
	ToggleInteractive key.Binding
	ToggleInnovate    key.Binding
	ToggleAutofill    key.Binding
	RunSpecs          key.Binding
	StopSpecs         key.Binding
	RunLoopOnce       key.Binding
	RunLoopContinuous key.Binding
	StopLoop          key.Binding
	EditLoopConfig    key.Binding
}

func newKeyMap() keyMap {
	return keyMap{
		Quit: key.NewBinding(
			key.WithKeys("q", "ctrl+c"),
			key.WithHelp("q", "quit"),
		),
		Focus: key.NewBinding(
			key.WithKeys("tab", "ctrl+f"),
			key.WithHelp("tab/ctrl+f", "toggle focus"),
		),
		Help: key.NewBinding(
			key.WithKeys("?"),
			key.WithHelp("?", "toggle help"),
		),
		RefreshNow: key.NewBinding(
			key.WithKeys("ctrl+l"),
			key.WithHelp("ctrl+l", "refresh now"),
		),
		Select: key.NewBinding(
			key.WithKeys("enter"),
			key.WithHelp("enter", "open"),
		),
		EditSpecsSettings: key.NewBinding(
			key.WithKeys("e"),
			key.WithHelp("e", "specs settings"),
		),
		ToggleLogsFormat: key.NewBinding(
			key.WithKeys("f"),
			key.WithHelp("f", "toggle logs format"),
		),
		SaveGlobal: key.NewBinding(
			key.WithKeys("ctrl+g"),
			key.WithHelp("ctrl+g", "save global"),
		),
		SaveRepo: key.NewBinding(
			key.WithKeys("ctrl+r"),
			key.WithHelp("ctrl+r", "save repo"),
		),
		Discard: key.NewBinding(
			key.WithKeys("ctrl+d"),
			key.WithHelp("ctrl+d", "discard session"),
		),
		ValidatePin: key.NewBinding(
			key.WithKeys("v"),
			key.WithHelp("v", "validate pin"),
		),
		MoveChecked: key.NewBinding(
			key.WithKeys("m"),
			key.WithHelp("m", "move checked"),
		),
		BlockItem: key.NewBinding(
			key.WithKeys("b"),
			key.WithHelp("b", "block item"),
		),
		ToggleChecked: key.NewBinding(
			key.WithKeys("x"),
			key.WithHelp("x", "toggle checked"),
		),
		TogglePane: key.NewBinding(
			key.WithKeys("ctrl+t"),
			key.WithHelp("ctrl+t", "toggle pane"),
		),
		ToggleInteractive: key.NewBinding(
			key.WithKeys("i"),
			key.WithHelp("i", "toggle interactive"),
		),
		ToggleInnovate: key.NewBinding(
			key.WithKeys("n"),
			key.WithHelp("n", "toggle innovate"),
		),
		ToggleAutofill: key.NewBinding(
			key.WithKeys("a"),
			key.WithHelp("a", "toggle autofill scout"),
		),
		RunSpecs: key.NewBinding(
			key.WithKeys("r"),
			key.WithHelp("r", "run specs build"),
		),
		StopSpecs: key.NewBinding(
			key.WithKeys("s"),
			key.WithHelp("s", "stop specs build"),
		),
		RunLoopOnce: key.NewBinding(
			key.WithKeys("r"),
			key.WithHelp("r", "run once"),
		),
		RunLoopContinuous: key.NewBinding(
			key.WithKeys("c"),
			key.WithHelp("c", "run continuous"),
		),
		StopLoop: key.NewBinding(
			key.WithKeys("s"),
			key.WithHelp("s", "stop loop"),
		),
		EditLoopConfig: key.NewBinding(
			key.WithKeys("e"),
			key.WithHelp("e", "edit loop overrides"),
		),
	}
}

func (k keyMap) ShortHelp() []key.Binding {
	return []key.Binding{k.Quit, k.Focus, k.Help, k.RefreshNow, k.Select}
}

func (k keyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{
		{k.Quit, k.Focus, k.Help, k.RefreshNow, k.Select},
		{k.EditSpecsSettings, k.ToggleLogsFormat, k.SaveGlobal, k.SaveRepo},
		{k.Discard},
		{k.ValidatePin, k.MoveChecked, k.BlockItem},
		{k.ToggleInteractive, k.ToggleInnovate, k.ToggleAutofill, k.RunSpecs, k.StopSpecs},
		{k.RunLoopOnce, k.RunLoopContinuous, k.StopLoop, k.EditLoopConfig},
	}
}
