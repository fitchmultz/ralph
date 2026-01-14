// Package tui provides rendering helpers for the Bubble Tea model.
// Entrypoint: withFinalNewline.
package tui

import (
	"strings"

	"github.com/charmbracelet/x/ansi"
)

// withFinalNewline preserves leading/trailing spaces but ensures exactly one trailing newline.
// TUIs treat whitespace as layout; never TrimSpace rendered output.
func withFinalNewline(s string) string {
	s = strings.TrimRight(s, "\n")
	return s + "\n"
}

// clampToSize ensures the rendered output never exceeds the provided width or height.
func clampToSize(s string, width int, height int) string {
	s = strings.TrimRight(s, "\n")
	if s == "" {
		return ""
	}
	lines := strings.Split(s, "\n")
	if height > 0 && len(lines) > height {
		lines = lines[:height]
	}
	if width > 0 {
		for i, line := range lines {
			lines[i] = ansi.Truncate(line, width, "")
		}
	}
	return strings.Join(lines, "\n")
}

// clipToHeight keeps at most height lines without width truncation.
func clipToHeight(s string, height int) string {
	s = strings.TrimRight(s, "\n")
	if s == "" || height <= 0 {
		return ""
	}
	lines := strings.Split(s, "\n")
	if len(lines) > height {
		lines = lines[:height]
	}
	return strings.Join(lines, "\n")
}
