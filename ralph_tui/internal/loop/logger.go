// Package loop provides logging helpers with env redaction.
// Entrypoint: Redactor, Logger.
package loop

import (
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

// Logger receives loop log lines.
type Logger interface {
	WriteLine(line string)
}

// Redactor redacts environment values from log lines.
type Redactor struct {
	keys   []string
	values []string
}

// NewRedactor builds a redactor from environment entries (KEY=VALUE).
func NewRedactor(env []string, mode redaction.Mode) *Redactor {
	mode = redaction.CoerceMode(string(mode))
	if mode == redaction.ModeOff {
		return nil
	}
	keys := make([]string, 0, len(env))
	values := make([]string, 0, len(env))
	for _, entry := range env {
		parts := strings.SplitN(entry, "=", 2)
		if len(parts) != 2 {
			continue
		}
		key := strings.TrimSpace(parts[0])
		value := parts[1]
		if key == "" || value == "" {
			continue
		}
		if redaction.IsPathLikeEnvKey(key) {
			continue
		}
		if mode == redaction.ModeSecretsOnly && !redaction.LooksSensitiveEnvKey(key) {
			continue
		}
		keys = append(keys, key)
		values = append(values, value)
	}
	if len(keys) == 0 {
		return nil
	}
	return &Redactor{keys: keys, values: values}
}

// Redact removes env-like content from the input line.
func (r *Redactor) Redact(line string) string {
	if r == nil || line == "" {
		return line
	}
	redacted := line
	for i, key := range r.keys {
		redacted = redactKeyValue(redacted, key)
		if value := r.values[i]; value != "" && len(value) >= 4 {
			redacted = strings.ReplaceAll(redacted, value, "[REDACTED]")
		}
	}
	return redacted
}

func redactKeyValue(line string, key string) string {
	needle := key + "="
	for {
		idx := strings.Index(line, needle)
		if idx == -1 {
			return line
		}
		start := idx + len(needle)
		end := start
		for end < len(line) {
			ch := line[end]
			if ch == ' ' || ch == '\t' || ch == '\n' {
				break
			}
			end++
		}
		line = line[:start] + "[REDACTED]" + line[end:]
	}
}
