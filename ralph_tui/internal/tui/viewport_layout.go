// Package tui provides shared sizing helpers for viewports.
// Entrypoint: resizeViewportToFit.
package tui

import (
	"github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/lipgloss"
)

var paddedViewportStyle = lipgloss.NewStyle().Padding(0, 1)

// resizeViewportToFit sets vp.Style and sizes the viewport to fit inside the provided outer bounds.
func resizeViewportToFit(vp *viewport.Model, outerW, outerH int, style lipgloss.Style) {
	if vp == nil {
		return
	}
	vp.Style = style
	frameW, frameH := style.GetFrameSize()
	vp.Width = max(0, outerW-frameW)
	vp.Height = max(0, outerH-frameH)
}
