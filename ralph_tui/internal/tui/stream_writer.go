// Package tui provides stream helpers for TUI log capture.
package tui

import (
	"github.com/mitchfultz/ralph/ralph_tui/internal/streaming"
)

const logChannelBufferSize = 4096

type lineSink interface {
	PushLine(line string)
}

type streamWriter struct {
	sink     lineSink
	splitter *streaming.LineSplitter
}

type logBatch struct {
	RunID int
	Lines []string
	Done  bool
}

func newStreamWriter(sink lineSink) *streamWriter {
	return &streamWriter{sink: sink, splitter: streaming.NewLineSplitter(streaming.DefaultMaxBufferedBytes)}
}

func newLogChannel() chan string {
	return make(chan string, logChannelBufferSize)
}

func sendLineBlocking(ch chan<- string, line string) {
	if ch == nil {
		return
	}
	ch <- line
}

func sendLineBestEffort(ch chan string, line string) {
	if ch == nil {
		return
	}
	select {
	case ch <- line:
		return
	default:
	}
	select {
	case <-ch:
	default:
	}
	select {
	case ch <- line:
	default:
	}
}

func (w *streamWriter) Write(p []byte) (int, error) {
	if w == nil {
		return len(p), nil
	}
	if w.splitter == nil {
		w.splitter = streaming.NewLineSplitter(streaming.DefaultMaxBufferedBytes)
	}
	w.splitter.Write(p, w.emitLine)
	return len(p), nil
}

func (w *streamWriter) Flush() {
	if w == nil {
		return
	}
	if w.splitter == nil {
		w.splitter = streaming.NewLineSplitter(streaming.DefaultMaxBufferedBytes)
	}
	w.splitter.Flush(w.emitLine)
}

func (w *streamWriter) emitLine(line string) {
	if w.sink != nil {
		w.sink.PushLine(line)
	}
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
