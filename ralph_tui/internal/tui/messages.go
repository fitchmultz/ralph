// Package tui defines shared message types for TUI event handling.
package tui

// specsUserFocusUpdatedMsg is emitted when the Specs user focus editor is saved.
type specsUserFocusUpdatedMsg struct {
	UserFocus string
}
