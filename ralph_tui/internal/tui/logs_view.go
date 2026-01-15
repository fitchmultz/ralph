// Package tui provides the log viewer screen for recent activity.
package tui

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
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
	viewport                viewport.Model
	logPath                 string
	logErr                  string
	lastStamp               fileStamp
	debugLines              []string
	loopLines               []string
	specsLines              []string
	format                  logsFormat
	width                   int
	height                  int
	lastRenderedContent     string
	viewportSetContentCalls int
}

func newLogsView(logPath string) *logsView {
	return &logsView{
		viewport: viewport.New(80, 20),
		logPath:  logPath,
		format:   logsFormatRaw,
	}
}

func (l *logsView) SetLogPath(path string) {
	if l.logPath == path {
		return
	}
	l.logPath = path
	l.logErr = ""
	l.lastStamp = fileStamp{}
	l.debugLines = nil
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
	l.setViewportContentIfChanged(l.renderContent(), atBottom)
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
	resizeViewportToFit(&l.viewport, max(0, width), max(0, contentHeight), paddedViewportStyle)
}

func (l *logsView) Refresh(loopLines []string, specsLines []string) {
	atBottom := l.viewport.AtBottom()

	l.loopLines = tailLines(loopLines, logsTailLines)
	l.specsLines = tailLines(specsLines, logsTailLines)

	if strings.TrimSpace(l.logPath) == "" {
		l.debugLines = nil
	} else {
		stamp, changed, err := fileChanged(l.logPath, l.lastStamp)
		if err != nil {
			l.logErr = err.Error()
		} else if changed || l.logErr != "" {
			lines, err := tailFileLines(l.logPath, logsTailLines)
			if err != nil {
				l.logErr = err.Error()
			} else {
				l.logErr = ""
				l.debugLines = lines
				l.lastStamp = stamp
			}
		}
	}

	rendered := l.renderContent()
	l.setViewportContentIfChanged(rendered, atBottom)
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
		l.renderLines(l.debugLines, "No log entries yet."),
		"",
		"Loop Output (tail)",
		l.renderLines(l.loopLines, "No loop output yet."),
		"",
		"Specs Output (tail)",
		l.renderLines(l.specsLines, "No specs output yet."),
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
	if limit <= 0 {
		return []string{}, nil
	}
	file, err := os.Open(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return []string{}, nil
		}
		return nil, err
	}
	defer file.Close()

	info, err := file.Stat()
	if err != nil {
		return nil, err
	}
	if info.Size() == 0 {
		return []string{}, nil
	}

	const chunkSize int64 = 64 * 1024
	pos := info.Size()
	newlineCount := 0
	chunks := make([][]byte, 0, 8)
	trimTrailing := true

	for pos > 0 && newlineCount < limit+1 {
		readLen := chunkSize
		if pos < readLen {
			readLen = pos
		}
		pos -= readLen
		if _, err := file.Seek(pos, io.SeekStart); err != nil {
			return nil, err
		}

		buf := make([]byte, int(readLen))
		if _, err := io.ReadFull(file, buf); err != nil {
			return nil, err
		}

		if trimTrailing {
			buf = bytes.TrimRight(buf, "\n")
			if len(buf) == 0 {
				continue
			}
			trimTrailing = false
		}

		newlineCount += bytes.Count(buf, []byte{'\n'})
		chunks = append(chunks, buf)
	}

	if len(chunks) == 0 {
		return []string{}, nil
	}

	totalLen := 0
	for _, chunk := range chunks {
		totalLen += len(chunk)
	}
	data := make([]byte, 0, totalLen)
	for i := len(chunks) - 1; i >= 0; i-- {
		data = append(data, chunks[i]...)
	}
	if len(data) == 0 {
		return []string{}, nil
	}

	content := string(data)
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

func (l *logsView) renderLines(lines []string, fallback string) string {
	if len(lines) == 0 {
		return fallback
	}
	if l.format == logsFormatRaw {
		return strings.Join(lines, "\n")
	}
	formatted := formatLogLines(lines)
	if len(formatted) == 0 {
		return fallback
	}
	return strings.Join(formatted, "\n")
}

func (l *logsView) setViewportContentIfChanged(content string, wasAtBottom bool) {
	if content == l.lastRenderedContent {
		return
	}
	l.viewport.SetContent(content)
	l.viewportSetContentCalls++
	l.lastRenderedContent = content
	if wasAtBottom {
		l.viewport.GotoBottom()
	}
}

func formatLogLines(lines []string) []string {
	formatted := make([]string, 0, len(lines))
	for _, line := range lines {
		formatted = append(formatted, formatLogLine(line))
	}
	return formatted
}

func formatLogLine(line string) string {
	trimmed := strings.TrimSpace(line)
	if trimmed == "" {
		return line
	}
	var entry logEntry
	if err := json.Unmarshal([]byte(trimmed), &entry); err != nil {
		return line
	}
	if entry.Message == "" && entry.Level == "" && entry.Timestamp == "" {
		return line
	}
	timestamp := formatLogTimestamp(entry.Timestamp)
	level := strings.ToUpper(strings.TrimSpace(entry.Level))
	message := strings.TrimSpace(entry.Message)
	fields := formatLogFields(entry.Fields)

	parts := make([]string, 0, 4)
	if timestamp != "" {
		parts = append(parts, timestamp)
	}
	if level != "" {
		parts = append(parts, level)
	}
	if message != "" {
		parts = append(parts, message)
	}
	lineOut := strings.Join(parts, " ")
	if fields != "" {
		if lineOut == "" {
			lineOut = fields
		} else {
			lineOut = lineOut + " | " + fields
		}
	}
	if lineOut == "" {
		return line
	}
	return lineOut
}

func formatLogTimestamp(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	parsed, err := time.Parse(time.RFC3339Nano, raw)
	if err != nil {
		return raw
	}
	return parsed.UTC().Format("2006-01-02 15:04:05Z")
}

func formatLogFields(fields map[string]any) string {
	if len(fields) == 0 {
		return ""
	}
	keys := make([]string, 0, len(fields))
	for key := range fields {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	parts := make([]string, 0, len(keys))
	for _, key := range keys {
		value := formatLogFieldValue(fields[key])
		if value == "" {
			parts = append(parts, key+"=")
			continue
		}
		parts = append(parts, fmt.Sprintf("%s=%s", key, value))
	}
	return strings.Join(parts, " ")
}

func formatLogFieldValue(value any) string {
	switch typed := value.(type) {
	case string:
		if strings.ContainsAny(typed, " \t\n") {
			return strconv.Quote(typed)
		}
		return typed
	default:
		return fmt.Sprint(value)
	}
}
