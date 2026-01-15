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

	repoLines := dashboardRepoLines(m)
	if len(repoLines) > 0 {
		lines = append(lines, "")
		lines = append(lines, "Repo:")
		lines = append(lines, repoLines...)
	}

	statusLines := dashboardStatusLines(m.pinView, m.loopView, m.specsView, m.fixup)
	if len(statusLines) > 0 {
		lines = append(lines, "")
		lines = append(lines, "Status:")
		lines = append(lines, statusLines...)
	}

	lines = append(lines, "")
	lines = append(lines, "Actions: r loop once | f fixup blocked | b build specs")

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

func dashboardStatusLines(p *pinView, l *loopView, s *specsView, f fixupState) []string {
	lines := make([]string, 0)
	if status := dashboardPinStatus(p); status != "" {
		lines = append(lines, "Pin: "+status)
	}
	if status := dashboardLoopStatus(l); status != "" {
		lines = append(lines, "Loop: "+status)
	}
	if status := dashboardLoopFailure(l); status != "" {
		lines = append(lines, "Loop failure: "+status)
	}
	if status := dashboardFixupStatus(f); status != "" {
		lines = append(lines, "Fixup: "+status)
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

func dashboardLoopFailure(l *loopView) string {
	if l == nil {
		return ""
	}
	stage := strings.TrimSpace(l.state.LastFailureStage)
	message := strings.TrimSpace(l.state.LastFailureMessage)
	if stage == "" && message == "" {
		return ""
	}
	if stage == "" {
		stage = "unknown"
	}
	if message == "" {
		return stage
	}
	return fmt.Sprintf("%s — %s", stage, message)
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

func dashboardFixupStatus(f fixupState) string {
	if f.running {
		return "Running..."
	}
	if f.err != "" {
		if f.hasSummary {
			return "Error: " + f.err + " | " + formatFixupSummary(f.summary)
		}
		return "Error: " + f.err
	}
	if f.hasSummary {
		return formatFixupSummary(f.summary)
	}
	return ""
}

func dashboardRepoLines(m model) []string {
	if strings.TrimSpace(m.locations.RepoRoot) == "" {
		return []string{"Unavailable: repo root unknown"}
	}
	if m.repoStatusErr != "" {
		return []string{"Unavailable: " + m.repoStatusErr}
	}
	if isRepoStatusEmpty(m.repoStatus) {
		return []string{"Loading repo status..."}
	}

	lines := make([]string, 0, 6)

	branch := m.repoStatus.Branch
	if strings.TrimSpace(branch) == "" {
		branch = "unknown"
	}
	if m.repoStatus.BranchNote != "" {
		branch = fmt.Sprintf("%s (%s)", branch, m.repoStatus.BranchNote)
	}
	head := strings.TrimSpace(m.repoStatus.ShortHead)
	if head == "" && m.repoStatus.ShortHeadNote != "" {
		head = m.repoStatus.ShortHeadNote
	}
	if head != "" {
		lines = append(lines, fmt.Sprintf("Branch: %s | HEAD: %s", branch, head))
	} else {
		lines = append(lines, fmt.Sprintf("Branch: %s", branch))
	}

	statusLine := strings.TrimSpace(m.repoStatus.StatusSummary)
	if statusLine == "" {
		if m.repoStatus.StatusSummaryNote != "" {
			statusLine = "unavailable (" + m.repoStatus.StatusSummaryNote + ")"
		} else {
			statusLine = "unknown"
		}
	}
	lines = append(lines, "Status: "+statusLine)

	if m.repoStatus.StatusSummary != "" {
		lines = append(lines, fmt.Sprintf("Dirty: %d file(s)", m.repoStatus.DirtyCount))
	} else if m.repoStatus.StatusSummaryNote != "" {
		lines = append(lines, fmt.Sprintf("Dirty: unavailable (%s)", m.repoStatus.StatusSummaryNote))
	} else {
		lines = append(lines, "Dirty: unknown")
	}

	if m.repoStatus.AheadNote != "" {
		lines = append(lines, fmt.Sprintf("Ahead: unavailable (%s)", m.repoStatus.AheadNote))
	} else {
		lines = append(lines, fmt.Sprintf("Ahead: %d", m.repoStatus.AheadCount))
	}

	if m.repoStatus.LastCommit != "" {
		lines = append(lines, "Last commit: "+m.repoStatus.LastCommit)
	} else if m.repoStatus.LastCommitNote != "" {
		lines = append(lines, "Last commit: unavailable ("+m.repoStatus.LastCommitNote+")")
	} else {
		lines = append(lines, "Last commit: unknown")
	}

	if m.repoStatus.LastCommitStat != "" {
		lines = append(lines, "Last diffstat: "+m.repoStatus.LastCommitStat)
	} else if m.repoStatus.LastCommitStatNote != "" {
		lines = append(lines, "Last diffstat: unavailable ("+m.repoStatus.LastCommitStatNote+")")
	} else {
		lines = append(lines, "Last diffstat: unknown")
	}

	return lines
}

func isRepoStatusEmpty(status repoStatusSnapshot) bool {
	return strings.TrimSpace(status.Branch) == "" &&
		strings.TrimSpace(status.BranchNote) == "" &&
		strings.TrimSpace(status.ShortHead) == "" &&
		strings.TrimSpace(status.ShortHeadNote) == "" &&
		strings.TrimSpace(status.StatusSummary) == "" &&
		strings.TrimSpace(status.StatusSummaryNote) == "" &&
		status.DirtyCount == 0 &&
		status.AheadCount == 0 &&
		strings.TrimSpace(status.AheadNote) == "" &&
		strings.TrimSpace(status.LastCommit) == "" &&
		strings.TrimSpace(status.LastCommitNote) == "" &&
		strings.TrimSpace(status.LastCommitStat) == "" &&
		strings.TrimSpace(status.LastCommitStatNote) == ""
}
