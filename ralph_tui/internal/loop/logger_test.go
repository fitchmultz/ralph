// Package loop provides tests for redaction helpers.
// Entrypoint: go test ./...
package loop

import (
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

func TestRedactorOff(t *testing.T) {
	env := testEnv()
	redactor := NewRedactor(env, redaction.ModeOff)
	if redactor != nil {
		t.Fatalf("expected nil redactor in off mode")
	}

	line := "export API_KEY=abcd1234"
	if got := redactWith(redactor, line); got != line {
		t.Fatalf("expected no redaction in off mode, got %q", got)
	}
}

func TestRedactorSecretsOnly(t *testing.T) {
	redactor := NewRedactor(testEnv(), redaction.ModeSecretsOnly)
	if redactor == nil {
		t.Fatalf("expected redactor in secrets_only mode")
	}

	assertRedaction(t, redactor, "export API_KEY=abcd1234", "export API_KEY=[REDACTED]")
	assertRedaction(t, redactor, "Authorization: Bearer abcd1234", "Authorization: Bearer [REDACTED]")
	assertRedaction(t, redactor, "API_KEY=abcd1234 API_KEY=abcd1234", "API_KEY=[REDACTED] API_KEY=[REDACTED]")
	assertRedaction(t, redactor, "FOO=barbaz", "FOO=barbaz")
	assertRedaction(t, redactor, "MONKEY=bananas", "MONKEY=bananas")
	assertRedaction(t, redactor, "PATH=/usr/bin:/bin", "PATH=/usr/bin:/bin")
	assertRedaction(t, redactor, "cd /Users/alice", "cd /Users/alice")
}

func TestRedactorAllEnv(t *testing.T) {
	redactor := NewRedactor(testEnv(), redaction.ModeAllEnv)
	if redactor == nil {
		t.Fatalf("expected redactor in all_env mode")
	}

	assertRedaction(t, redactor, "export API_KEY=abcd1234", "export API_KEY=[REDACTED]")
	assertRedaction(t, redactor, "FOO=barbaz", "FOO=[REDACTED]")
	assertRedaction(t, redactor, "PATH=/usr/bin:/bin", "PATH=/usr/bin:/bin")
	assertRedaction(t, redactor, "cd /Users/alice", "cd /Users/alice")
}

func testEnv() []string {
	return []string{
		"PATH=/usr/bin:/bin",
		"HOME=/Users/alice",
		"CWD=/tmp/project",
		"API_KEY=abcd1234",
		"DB_PASSWORD=supersecret",
		"FOO=barbaz",
		"MONKEY=bananas",
	}
}

func assertRedaction(t *testing.T, redactor *Redactor, line string, expected string) {
	t.Helper()
	if got := redactWith(redactor, line); got != expected {
		t.Fatalf("expected %q, got %q", expected, got)
	}
}

func redactWith(redactor *Redactor, line string) string {
	if redactor == nil {
		return line
	}
	return redactor.Redact(line)
}
