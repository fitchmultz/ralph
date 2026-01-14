// Package loop provides standard output logging.
// Entrypoint: StdLogger.
package loop

import "io"

// StdLogger writes loop logs to an io.Writer.
type StdLogger struct {
	Writer io.Writer
}

// WriteLine writes a line to the writer.
func (s StdLogger) WriteLine(line string) {
	if s.Writer == nil {
		return
	}
	_, _ = io.WriteString(s.Writer, line+"\n")
}
