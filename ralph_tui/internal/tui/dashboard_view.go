// Package tui provides the Dashboard screen rendering.
// Entrypoint: model.dashboardView.
package tui

import (
	"fmt"
	"strings"
)

type dashboardQueueSummary struct {
	summary string
	nextID  string
	lastID  string
}

func (m model) dashboardView() string {
	lines := []string{"Dashboard"}

	queueSummary := dashboardQueueSummaryFor(m.pinView)
	lines = append(lines, fmt.Sprintf("Queue: %s", queueSummary.summary))
	if queueSummary.nextID != "" {
		lines = append(lines, fmt.Sprintf("Next item: %s", queueSummary.nextID))
	}
	if queueSummary.lastID != "" {
		lines = append(lines, fmt.Sprintf("Last done: %s", queueSummary.lastID))
	}

	lines = append(lines, fmt.Sprintf("Loop: %s", dashboardLoopState(m.loopView)))
	lines = append(lines, fmt.Sprintf("Specs: %s", dashboardSpecsState(m.specsView)))
	lines = append(lines, fmt.Sprintf("Log path: %s", dashboardLogPath(m)))

	statusLines := dashboardStatusLines(m.pinView, m.loopView, m.specsView)
	if len(statusLines) > 0 {
		lines = append(lines, "")
		lines = append(lines, "Status:")
		lines = append(lines, statusLines...)
	}

	lines = append(lines, "")
	lines = append(lines, "Actions: r loop once | b build specs")

	return withFinalNewline(strings.Join(lines, "\n"))
}

func dashboardQueueSummaryFor(p *pinView) dashboardQueueSummary {
	if p == nil {
		return dashboardQueueSummary{summary: "unavailable"}
	}
	if p.loading {
		return dashboardQueueSummary{summary: "loading"}
	}
	if p.err != "" {
		return dashboardQueueSummary{summary: "error"}
	}

	total := len(p.items)
	checked := 0
	nextID := ""
	lastID := ""
	for _, item := range p.items {
		if item.Checked {
			checked++
			lastID = item.ID
			continue
		}
		if nextID == "" {
			nextID = item.ID
		}
	}
	unchecked := total - checked
	summary := fmt.Sprintf("%d total | %d unchecked | %d blocked", total, unchecked, p.blockedCount)
	return dashboardQueueSummary{summary: summary, nextID: nextID, lastID: lastID}
}

func dashboardLoopState(l *loopView) string {
	if l == nil {
		return "unavailable"
	}
	switch l.mode {
	case loopIdle:
		return "idle"
	case loopRunning:
		return "running"
	case loopStopping:
		return "stopping"
	case loopEditing:
		return "editing"
	default:
		return "unknown"
	}
}

func dashboardSpecsState(s *specsView) string {
	if s == nil {
		return "unavailable"
	}
	if s.running {
		return "running"
	}
	return "idle"
}

func dashboardLogPath(m model) string {
	if m.logger != nil && m.logger.Path() != "" {
		return m.logger.Path()
	}
	path, err := resolveLogPath(m.cfg)
	if err != nil {
		return "unavailable"
	}
	return path
}

func dashboardStatusLines(p *pinView, l *loopView, s *specsView) []string {
	lines := make([]string, 0)
	if status := dashboardPinStatus(p); status != "" {
		lines = append(lines, "Pin: "+status)
	}
	if status := dashboardLoopStatus(l); status != "" {
		lines = append(lines, "Loop: "+status)
	}
	if status := dashboardSpecsStatus(s); status != "" {
		lines = append(lines, "Specs: "+status)
	}
	return lines
}

func dashboardPinStatus(p *pinView) string {
	if p == nil {
		return ""
	}
	if p.loading {
		return "Loading pin..."
	}
	if p.err != "" {
		return "Error: " + p.err
	}
	return p.status
}

func dashboardLoopStatus(l *loopView) string {
	if l == nil {
		return ""
	}
	if l.err != "" {
		return "Error: " + l.err
	}
	if l.outputErr != "" {
		return l.status + " | Persist error: " + l.outputErr
	}
	return l.status
}

func dashboardSpecsStatus(s *specsView) string {
	if s == nil {
		return ""
	}
	if s.err != "" {
		return "Error: " + s.err
	}
	if s.refreshErr != "" {
		return "Error: " + s.refreshErr
	}
	if s.persistErr != "" {
		return "Persist error: " + s.persistErr
	}
	if s.status != "" {
		return s.status
	}
	if s.previewLoading && !s.running {
		return "Rendering preview..."
	}
	return ""
}
