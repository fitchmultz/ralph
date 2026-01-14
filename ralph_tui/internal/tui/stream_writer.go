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
