// Package loop provides a line-oriented writer for streaming logs.
// Entrypoint: lineWriter.
package loop

import (
	"io"
	"strings"
	"sync"
)

type lineWriter struct {
	redactor *Redactor
	logger   Logger
	tee      io.Writer
	buffer   string
	mu       sync.Mutex
}

func newLineWriter(redactor *Redactor, logger Logger, tee io.Writer) *lineWriter {
	return &lineWriter{redactor: redactor, logger: logger, tee: tee}
}

func (w *lineWriter) Write(p []byte) (int, error) {
	w.mu.Lock()
	defer w.mu.Unlock()

	chunk := w.buffer + string(p)
	lines := strings.Split(chunk, "\n")
	w.buffer = lines[len(lines)-1]
	lines = lines[:len(lines)-1]
	for _, line := range lines {
		w.writeLine(line)
	}
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
	if w.buffer == "" {
		return
	}
	w.writeLine(w.buffer)
	w.buffer = ""
}
