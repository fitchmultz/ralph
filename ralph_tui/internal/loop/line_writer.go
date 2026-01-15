// Package loop provides a line-oriented writer for streaming logs.
// Entrypoint: lineWriter.
package loop

import (
	"io"
	"sync"

	"github.com/mitchfultz/ralph/ralph_tui/internal/streaming"
)

type lineWriter struct {
	redactor *Redactor
	logger   Logger
	tee      io.Writer
	splitter *streaming.LineSplitter
	mu       sync.Mutex
}

func newLineWriter(redactor *Redactor, logger Logger, tee io.Writer) *lineWriter {
	return &lineWriter{redactor: redactor, logger: logger, tee: tee, splitter: streaming.NewLineSplitter(streaming.DefaultMaxBufferedBytes)}
}

func (w *lineWriter) Write(p []byte) (int, error) {
	w.mu.Lock()
	defer w.mu.Unlock()

	if w.splitter == nil {
		w.splitter = streaming.NewLineSplitter(streaming.DefaultMaxBufferedBytes)
	}
	w.splitter.Write(p, w.writeLine)
	return len(p), nil
}

func (w *lineWriter) writeLine(line string) {
	clean := line
	if w.redactor != nil {
		clean = w.redactor.Redact(clean)
	}
	if w.logger != nil {
		w.logger.WriteLine(clean)
	}
	if w.tee != nil {
		_, _ = io.WriteString(w.tee, clean+"\n")
	}
}

func (w *lineWriter) Flush() {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.splitter == nil {
		w.splitter = streaming.NewLineSplitter(streaming.DefaultMaxBufferedBytes)
	}
	w.splitter.Flush(w.writeLine)
}
