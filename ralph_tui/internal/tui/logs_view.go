// Package tui provides the log viewer screen for recent activity.
package tui

import (
	"errors"
	"os"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

const (
	logsTailLines = 200
)

type logsFormat int

const (
	logsFormatRaw logsFormat = iota
	logsFormatFormatted
)

type logsView struct {
	viewport   viewport.Model
	logPath    string
	logErr     string
	debugLines []string
	loopLines  []string
	specsLines []string
	format     logsFormat
	width      int
	height     int
}

func newLogsView(logPath string) *logsView {
	return &logsView{
		viewport: viewport.New(80, 20),
		logPath:  logPath,
		format:   logsFormatRaw,
	}
}

func (l *logsView) SetLogPath(path string) {
	l.logPath = path
}

func (l *logsView) SetError(err error) {
	if err == nil {
		l.logErr = ""
		return
	}
	l.logErr = err.Error()
}

func (l *logsView) Update(msg tea.Msg) tea.Cmd {
	updated, cmd := l.viewport.Update(msg)
	l.viewport = updated
	return cmd
}

func (l *logsView) ToggleFormat() {
	atBottom := l.viewport.AtBottom()
	if l.format == logsFormatRaw {
		l.format = logsFormatFormatted
	} else {
		l.format = logsFormatRaw
	}
	l.viewport.SetContent(l.renderContent())
	if atBottom {
		l.viewport.GotoBottom()
	}
}

func (l *logsView) View() string {
	header := "Logs"
	status := l.statusLine()
	content := l.viewport.View()
	return withFinalNewline(header + "\n" + status + "\n\n" + content)
}

func (l *logsView) Resize(width int, height int) {
	l.width = width
	l.height = height
	contentHeight := height - 3
	if contentHeight < 0 {
		contentHeight = 0
	}
	l.viewport.Width = max(0, width)
	l.viewport.Height = max(0, contentHeight)
	l.viewport.Style = lipgloss.NewStyle().Padding(0, 1)
}

func (l *logsView) Refresh(loopLines []string, specsLines []string) {
	atBottom := l.viewport.AtBottom()

	l.loopLines = tailLines(loopLines, logsTailLines)
	l.specsLines = tailLines(specsLines, logsTailLines)
	l.debugLines = nil

	if strings.TrimSpace(l.logPath) != "" {
		lines, err := tailFileLines(l.logPath, logsTailLines)
		if err != nil {
			l.logErr = err.Error()
		} else {
			l.logErr = ""
			l.debugLines = lines
		}
	}

	l.viewport.SetContent(l.renderContent())
	if atBottom {
		l.viewport.GotoBottom()
	}
}

func (l *logsView) statusLine() string {
	if l.logErr != "" {
		return "Error: " + l.logErr
	}
	formatNote := "Format: " + l.formatLabel()
	if strings.TrimSpace(l.logPath) == "" {
		return "Log file unavailable. | " + formatNote
	}
	return "Log file: " + l.logPath + " | " + formatNote
}

func (l *logsView) renderContent() string {
	sections := []string{
		"Debug Log (tail)",
		linesOrFallback(l.debugLines, "No log entries yet."),
		"",
		"Loop Output (tail)",
		linesOrFallback(l.loopLines, "No loop output yet."),
		"",
		"Specs Output (tail)",
		linesOrFallback(l.specsLines, "No specs output yet."),
	}
	return strings.Join(sections, "\n")
}

func (l *logsView) formatLabel() string {
	if l.format == logsFormatFormatted {
		return "formatted"
	}
	return "raw"
}

func tailFileLines(path string, limit int) ([]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return []string{}, nil
		}
		return nil, err
	}
	content := strings.TrimRight(string(data), "\n")
	if content == "" {
		return []string{}, nil
	}
	return tailLines(strings.Split(content, "\n"), limit), nil
}

func tailLines(lines []string, limit int) []string {
	if limit <= 0 {
		return []string{}
	}
	if len(lines) <= limit {
		return append([]string{}, lines...)
	}
	return append([]string{}, lines[len(lines)-limit:]...)
}

func linesOrFallback(lines []string, fallback string) string {
	if len(lines) == 0 {
		return fallback
	}
	return strings.Join(lines, "\n")
}
