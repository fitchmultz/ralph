// Package redaction centralizes log redaction modes and env key classification.
// Entrypoint: CoerceMode, LooksSensitiveEnvKey.
package redaction

import (
	"regexp"
	"strings"
)

// Mode controls how environment values are redacted from logs.
type Mode string

const (
	ModeOff         Mode = "off"
	ModeSecretsOnly Mode = "secrets_only"
	ModeAllEnv      Mode = "all_env"
)

var (
	sensitiveKeyPattern = regexp.MustCompile(`(?i)(^|[_-])(key|secret|token|password)(\d*|[_-]|$)`)
	pathLikeEnvKeys     = map[string]struct{}{
		"CWD":    {},
		"HOME":   {},
		"OLDPWD": {},
		"PATH":   {},
		"PWD":    {},
		"TEMP":   {},
		"TMP":    {},
		"TMPDIR": {},
	}
)

// NormalizeMode trims and lowercases a redaction mode string.
func NormalizeMode(value string) Mode {
	return Mode(strings.ToLower(strings.TrimSpace(value)))
}

// CoerceMode returns a supported mode, defaulting to secrets_only on empty/invalid values.
func CoerceMode(value string) Mode {
	normalized := NormalizeMode(value)
	if normalized == "" || !ValidMode(string(normalized)) {
		return ModeSecretsOnly
	}
	return normalized
}

// ValidMode reports whether the value is a supported redaction mode.
func ValidMode(value string) bool {
	switch NormalizeMode(value) {
	case ModeOff, ModeSecretsOnly, ModeAllEnv:
		return true
	default:
		return false
	}
}

// IsPathLikeEnvKey reports whether an env key is a common path-like variable.
func IsPathLikeEnvKey(key string) bool {
	normalized := normalizeKey(key)
	_, ok := pathLikeEnvKeys[normalized]
	return ok
}

// LooksSensitiveEnvKey reports whether an env key name appears to hold secrets.
func LooksSensitiveEnvKey(key string) bool {
	return sensitiveKeyPattern.MatchString(normalizeKey(key))
}

func normalizeKey(key string) string {
	return strings.ToUpper(strings.TrimSpace(key))
}
