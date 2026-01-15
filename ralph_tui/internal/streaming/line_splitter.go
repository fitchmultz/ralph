// Package streaming provides shared helpers for line-oriented log streaming.
// Entrypoint: LineSplitter.
package streaming

// DefaultMaxBufferedBytes flushes partial lines once the buffer exceeds this size.
const DefaultMaxBufferedBytes = 512

// LineSplitter buffers byte streams into newline-delimited log lines.
// It treats both '\n' and '\r' as line terminators and can flush partial
// buffers once they exceed maxBufferedBytes.
type LineSplitter struct {
	buf              []byte
	pendingCR        bool
	maxBufferedBytes int
}

// NewLineSplitter builds a line splitter with the provided partial flush limit.
func NewLineSplitter(maxBufferedBytes int) *LineSplitter {
	return &LineSplitter{maxBufferedBytes: maxBufferedBytes}
}

// Write splits incoming bytes into lines and emits them via the callback.
func (s *LineSplitter) Write(p []byte, emit func(string)) {
	if s == nil || len(p) == 0 {
		return
	}
	start := 0
	if s.pendingCR {
		if p[0] == '\n' {
			start = 1
		}
		s.pendingCR = false
	}
	for i := start; i < len(p); i++ {
		b := p[i]
		if b != '\n' && b != '\r' {
			continue
		}
		s.buf = append(s.buf, p[start:i]...)
		s.emitLine(emit, true)
		if b == '\r' {
			if i+1 < len(p) && p[i+1] == '\n' {
				i++
			} else if i == len(p)-1 {
				s.pendingCR = true
			}
		}
		start = i + 1
	}
	if start < len(p) {
		s.buf = append(s.buf, p[start:]...)
		if s.maxBufferedBytes > 0 && len(s.buf) >= s.maxBufferedBytes {
			s.emitLine(emit, false)
		}
	}
}

// Flush emits any buffered partial line.
func (s *LineSplitter) Flush(emit func(string)) {
	if s == nil {
		return
	}
	s.emitLine(emit, false)
}

func (s *LineSplitter) emitLine(emit func(string), allowEmpty bool) {
	if len(s.buf) == 0 && !allowEmpty {
		return
	}
	line := ""
	if len(s.buf) > 0 {
		line = string(s.buf)
	}
	if emit != nil {
		emit(line)
	}
	s.buf = s.buf[:0]
}
