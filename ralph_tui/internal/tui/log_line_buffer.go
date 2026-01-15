// Package tui provides shared log buffering helpers for the TUI views.
package tui

import (
	"unsafe"
)

type logLineBuffer struct {
	maxLines int
	trimTo   int
	lines    []string
	joined   []byte
	shared   bool // true when ContentString shares joined bytes with a string.
	version  uint64
}

func newLogLineBuffer(maxLines int, trimTo int) logLineBuffer {
	buffer := logLineBuffer{maxLines: maxLines}
	if maxLines <= 0 {
		buffer.trimTo = 0
		return buffer
	}
	if trimTo <= 0 || trimTo > maxLines {
		trimTo = maxLines * 3 / 4
		if trimTo <= 0 {
			trimTo = maxLines
		}
	}
	buffer.trimTo = trimTo
	return buffer
}

func (b *logLineBuffer) Reset() {
	b.lines = nil
	b.joined = nil
	b.shared = false
	b.version++
}

func (b *logLineBuffer) AppendLines(lines []string) {
	if len(lines) == 0 {
		return
	}
	if b.shared {
		b.joined = append([]byte(nil), b.joined...)
		b.shared = false
	}
	b.lines = append(b.lines, lines...)
	for _, line := range lines {
		if len(b.joined) > 0 {
			b.joined = append(b.joined, '\n')
		}
		b.joined = append(b.joined, line...)
	}
	b.version++
	if b.maxLines <= 0 {
		return
	}
	if len(b.lines) > b.maxLines {
		trimTo := b.trimTo
		if trimTo <= 0 || trimTo > b.maxLines {
			trimTo = b.maxLines
		}
		drop := len(b.lines) - trimTo
		if drop > 0 {
			b.lines = b.lines[drop:]
			b.joined = buildJoinedBytes(b.lines)
			b.shared = false
			b.version++
		}
	}
}

func (b *logLineBuffer) Lines() []string {
	return b.lines
}

func (b *logLineBuffer) TailView(limit int) []string {
	return tailLines(b.lines, limit)
}

func (b *logLineBuffer) ContentString() string {
	if len(b.joined) == 0 {
		return ""
	}
	b.shared = true
	// AppendLines copies joined before mutation when shared to keep prior strings immutable.
	return bytesToImmutableString(b.joined)
}

func (b *logLineBuffer) Version() uint64 {
	return b.version
}

func bytesToImmutableString(buf []byte) string {
	if len(buf) == 0 {
		return ""
	}
	return unsafe.String(unsafe.SliceData(buf), len(buf))
}

func buildJoinedBytes(lines []string) []byte {
	if len(lines) == 0 {
		return nil
	}
	length := 0
	for _, line := range lines {
		length += len(line)
	}
	length += len(lines) - 1
	buf := make([]byte, 0, length)
	for i, line := range lines {
		if i > 0 {
			buf = append(buf, '\n')
		}
		buf = append(buf, line...)
	}
	return buf
}
