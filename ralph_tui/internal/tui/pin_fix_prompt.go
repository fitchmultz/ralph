// Package tui provides the pin duplicate fix prompt for startup validation.
// Entrypoint: pinFixPromptView.
package tui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

type pinFixPrompt struct {
	err     error
	report  pin.DuplicateIDReport
	running bool
}

type pinFixResultMsg struct {
	result pin.FixDuplicateIDsResult
	err    error
}

func fixPinDuplicatesCmd(files pin.Files, projectType project.Type) tea.Cmd {
	return func() tea.Msg {
		result, err := pin.FixDuplicateQueueIDs(files, "", projectType)
		return pinFixResultMsg{result: result, err: err}
	}
}

func pinFixPromptView(prompt pinFixPrompt) string {
	if prompt.running {
		return "Fixing duplicate pin IDs...\n"
	}

	lines := []string{
		"Duplicate pin IDs detected.",
	}
	if prompt.err != nil {
		lines = append(lines, fmt.Sprintf("Validation error: %v", prompt.err))
	}
	if len(prompt.report.All) > 0 {
		lines = append(lines, fmt.Sprintf("Duplicates: %s", strings.Join(prompt.report.All, ", ")))
	}
	lines = append(lines, "", "Fix queue IDs now? (y/n)")
	lines = append(lines, "Tip: run `ralph pin fix-ids` to do this manually.")
	return strings.Join(lines, "\n") + "\n"
}
