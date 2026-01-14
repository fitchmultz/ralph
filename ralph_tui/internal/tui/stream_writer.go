// Package tui provides stream helpers for TUI log capture.
package tui

import "strings"

type lineSink interface {
	PushLine(line string)
}

type streamWriter struct {
	sink lineSink
	buf  strings.Builder
}

type logBatch struct {
	RunID int
	Lines []string
	Done  bool
}

func newStreamWriter(sink lineSink) *streamWriter {
	return &streamWriter{sink: sink}
}

func (w *streamWriter) Write(p []byte) (int, error) {
	if w == nil {
		return len(p), nil
	}
	start := 0
	for i, b := range p {
		if b == '\n' {
			w.buf.Write(p[start:i])
			w.flushLine()
			start = i + 1
		}
	}
	if start < len(p) {
		w.buf.Write(p[start:])
	}
	return len(p), nil
}

func (w *streamWriter) Flush() {
	if w == nil {
		return
	}
	if w.buf.Len() == 0 {
		return
	}
	w.flushLine()
}

func (w *streamWriter) flushLine() {
	if w.sink != nil {
		w.sink.PushLine(w.buf.String())
	}
	w.buf.Reset()
}

func drainLogChannel(runID int, logCh <-chan string, maxBatch int) logBatch {
	if maxBatch <= 0 {
		maxBatch = 64
	}
	line, ok := <-logCh
	if !ok {
		return logBatch{RunID: runID, Done: true}
	}
	lines := []string{line}
	for len(lines) < maxBatch {
		select {
		case line, ok := <-logCh:
			if !ok {
				return logBatch{RunID: runID, Lines: lines, Done: true}
			}
			lines = append(lines, line)
		default:
			return logBatch{RunID: runID, Lines: lines}
		}
	}
	return logBatch{RunID: runID, Lines: lines}
}
