// Package tui provides shared key hint rendering helpers for the Ralph TUI.
package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/key"
)

func renderKeyHints(label string, bindings []key.Binding) string {
	parts := make([]string, 0, len(bindings))
	for _, binding := range bindings {
		help := binding.Help()
		if help.Key == "" || help.Desc == "" {
			continue
		}
		parts = append(parts, fmt.Sprintf("%s %s", help.Key, help.Desc))
	}
	if len(parts) == 0 {
		return ""
	}
	return fmt.Sprintf("%s: %s", label, strings.Join(parts, " | "))
}

func dashboardActionBindings(keys keyMap) []key.Binding {
	return []key.Binding{keys.DashboardRunLoopOnce, keys.DashboardFixupBlocked, keys.DashboardBuildSpecs}
}

func dashboardActionLine(keys keyMap) string {
	return renderKeyHints("Actions", dashboardActionBindings(keys))
}

func specsKeyHintBindings(keys keyMap, running bool) []key.Binding {
	bindings := []key.Binding{keys.EditSpecsSettings}
	if !running {
		bindings = append(bindings,
			keys.ToggleInteractive,
			keys.ToggleInnovate,
			keys.ToggleAutofill,
			keys.ToggleScoutWorkflow,
			keys.EditUserFocus,
			keys.RunSpecs,
		)
	}
	if running {
		bindings = append(bindings, keys.StopSpecs)
	}
	return bindings
}

func specsKeyHelpGroups(keys keyMap, running bool) [][]key.Binding {
	if running {
		return [][]key.Binding{{keys.EditSpecsSettings, keys.StopSpecs}}
	}
	return [][]key.Binding{{
		keys.EditSpecsSettings,
		keys.ToggleInteractive,
		keys.ToggleInnovate,
		keys.ToggleAutofill,
		keys.ToggleScoutWorkflow,
		keys.EditUserFocus,
	}, {
		keys.RunSpecs,
	}}
}

func loopKeyHintBindings(keys keyMap, mode loopMode, supportsEffort bool) []key.Binding {
	bindings := make([]key.Binding, 0, 6)
	switch mode {
	case loopRunning:
		bindings = append(bindings, keys.StopLoop)
	case loopIdle:
		bindings = append(bindings, keys.RunLoopOnce, keys.RunLoopContinuous, keys.EditLoopConfig)
	case loopStopping:
		// Stop is already in progress; avoid advertising a disabled binding.
	}
	if supportsEffort && mode != loopEditing {
		bindings = append(bindings, keys.ToggleForceContextBuilder)
	}
	bindings = append(bindings, keys.JumpToPin, keys.JumpToLogs)
	return bindings
}
