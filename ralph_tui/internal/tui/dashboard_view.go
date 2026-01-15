// Package tui provides the Dashboard screen rendering.
// Entrypoint: model.dashboardView.
package tui

import (
	"fmt"
	"strings"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
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
	if actions := dashboardActionLine(m.keys); actions != "" {
		lines = append(lines, actions)
	}

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

	items := p.allItems
	if len(items) == 0 {
		items = p.items
	}
	total := len(items)
	checked := 0
	nextID := ""
	lastID := ""
	for _, item := range items {
		if item.Checked {
			checked++
			continue
		}
		if nextID == "" {
			nextID = item.ID
		}
	}
	if summary, err := pin.ReadDoneSummary(p.files.DonePath); err == nil {
		lastID = summary.LastID
	}
	unchecked := total - checked
	summary := fmt.Sprintf("%d queued | %d unchecked | %d blocked", total, unchecked, p.blockedCount)
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
	if m.repoStatus.Err != nil {
		line := "Unavailable: " + m.repoStatus.Err.Error()
		if !m.repoStatus.NextAllowedAt.IsZero() {
			remaining := time.Until(m.repoStatus.NextAllowedAt)
			if remaining > 0 {
				line = fmt.Sprintf("%s (retry in %s)", line, remaining.Round(time.Second))
			}
		}
		return []string{line}
	}
	if isRepoStatusEmpty(m.repoStatus.Snapshot) {
		return []string{"Loading repo status..."}
	}

	lines := make([]string, 0, 6)

	branch := m.repoStatus.Snapshot.Branch
	if strings.TrimSpace(branch) == "" {
		branch = "unknown"
	}
	if m.repoStatus.Snapshot.BranchNote != "" {
		branch = fmt.Sprintf("%s (%s)", branch, m.repoStatus.Snapshot.BranchNote)
	}
	head := strings.TrimSpace(m.repoStatus.Snapshot.ShortHead)
	if head == "" && m.repoStatus.Snapshot.ShortHeadNote != "" {
		head = m.repoStatus.Snapshot.ShortHeadNote
	}
	if head != "" {
		lines = append(lines, fmt.Sprintf("Branch: %s | HEAD: %s", branch, head))
	} else {
		lines = append(lines, fmt.Sprintf("Branch: %s", branch))
	}

	statusLine := strings.TrimSpace(m.repoStatus.Snapshot.StatusSummary)
	if statusLine == "" {
		if m.repoStatus.Snapshot.StatusSummaryNote != "" {
			statusLine = "unavailable (" + m.repoStatus.Snapshot.StatusSummaryNote + ")"
		} else {
			statusLine = "unknown"
		}
	}
	lines = append(lines, "Status: "+statusLine)

	if m.repoStatus.Snapshot.StatusSummary != "" {
		lines = append(lines, fmt.Sprintf("Dirty: %d file(s)", m.repoStatus.Snapshot.DirtyCount))
	} else if m.repoStatus.Snapshot.StatusSummaryNote != "" {
		lines = append(lines, fmt.Sprintf("Dirty: unavailable (%s)", m.repoStatus.Snapshot.StatusSummaryNote))
	} else {
		lines = append(lines, "Dirty: unknown")
	}

	if m.repoStatus.Snapshot.AheadNote != "" {
		lines = append(lines, fmt.Sprintf("Ahead: unavailable (%s)", m.repoStatus.Snapshot.AheadNote))
	} else {
		lines = append(lines, fmt.Sprintf("Ahead: %d", m.repoStatus.Snapshot.AheadCount))
	}

	if m.repoStatus.Snapshot.LastCommit != "" {
		lines = append(lines, "Last commit: "+m.repoStatus.Snapshot.LastCommit)
	} else if m.repoStatus.Snapshot.LastCommitNote != "" {
		lines = append(lines, "Last commit: unavailable ("+m.repoStatus.Snapshot.LastCommitNote+")")
	} else {
		lines = append(lines, "Last commit: unknown")
	}

	if m.repoStatus.Snapshot.LastCommitStat != "" {
		lines = append(lines, "Last diffstat: "+m.repoStatus.Snapshot.LastCommitStat)
	} else if m.repoStatus.Snapshot.LastCommitStatNote != "" {
		lines = append(lines, "Last diffstat: unavailable ("+m.repoStatus.Snapshot.LastCommitStatNote+")")
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
