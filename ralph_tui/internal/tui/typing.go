package tui

import tea "github.com/charmbracelet/bubbletea"

func isTextEntryKey(msg tea.KeyMsg) bool {
	switch msg.Type {
	case tea.KeyRunes, tea.KeySpace, tea.KeyBackspace, tea.KeyDelete:
		return true
	default:
		return false
	}
}

func isHardQuitKey(msg tea.KeyMsg) bool {
	return msg.Type == tea.KeyCtrlC
}
