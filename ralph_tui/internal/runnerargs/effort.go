// Package runnerargs centralizes runner argument parsing and reasoning effort handling.
// Entrypoint: ApplyReasoningEffort.
package runnerargs

import (
	"fmt"
	"strings"
)

// EffortSource notes where the effective reasoning effort came from.
type EffortSource string

const (
	EffortSourceArgs     EffortSource = "args"
	EffortSourceExplicit EffortSource = "explicit"
	EffortSourceAuto     EffortSource = "auto"
	EffortSourceNone     EffortSource = "none"
)

// EffortResult captures the applied arguments and effective effort.
type EffortResult struct {
	Args      []string
	Effective string
	Source    EffortSource
}

// SupportsReasoningEffort returns true when the runner supports reasoning effort settings.
func SupportsReasoningEffort(runner string) bool {
	return strings.ToLower(strings.TrimSpace(runner)) == "codex"
}

// ApplyReasoningEffort injects reasoning effort into Codex args when explicitly set.
func ApplyReasoningEffort(runner string, args []string, effort string) EffortResult {
	return ApplyReasoningEffortWithAutoTarget(runner, args, effort, "")
}

// ApplyReasoningEffortWithAutoTarget injects reasoning effort into Codex args.
// If effort is auto and autoTarget is set, the target is applied explicitly.
func ApplyReasoningEffortWithAutoTarget(runner string, args []string, effort string, autoTarget string) EffortResult {
	if !SupportsReasoningEffort(runner) {
		return EffortResult{Args: args, Effective: "", Source: EffortSourceNone}
	}

	if detected, ok := DetectEffort(args); ok {
		return EffortResult{Args: args, Effective: detected, Source: EffortSourceArgs}
	}

	normalized := NormalizeEffort(effort)
	if normalized == "" || normalized == "auto" {
		target := NormalizeEffort(autoTarget)
		if target != "" && target != "auto" {
			applied := append([]string{"-c", fmt.Sprintf("model_reasoning_effort=\"%s\"", target)}, args...)
			return EffortResult{Args: applied, Effective: target, Source: EffortSourceAuto}
		}
		return EffortResult{Args: args, Effective: "auto", Source: EffortSourceAuto}
	}

	applied := append([]string{"-c", fmt.Sprintf("model_reasoning_effort=\"%s\"", normalized)}, args...)
	return EffortResult{Args: applied, Effective: normalized, Source: EffortSourceExplicit}
}

// NormalizeEffort normalizes effort values for comparison and storage.
func NormalizeEffort(value string) string {
	return strings.ToLower(strings.TrimSpace(value))
}

// DisplayEffort normalizes effort values for user-friendly display.
func DisplayEffort(value string) string {
	normalized := NormalizeEffort(value)
	if normalized == "" || normalized == "auto" {
		return "auto"
	}
	return normalized
}

// DisplayEffortResult formats the effective effort for display.
func DisplayEffortResult(result EffortResult) string {
	if result.Source == EffortSourceNone {
		return "n/a"
	}
	if result.Effective == "" || result.Effective == "auto" {
		return "auto"
	}
	return result.Effective
}

// DetectEffort reads the effective effort from existing runner args.
func DetectEffort(args []string) (string, bool) {
	detected := ""
	for idx := 0; idx < len(args); idx++ {
		token := args[idx]
		if token == "-c" && idx+1 < len(args) {
			if value, ok := ExtractEffort(args[idx+1]); ok {
				detected = value
			}
			idx++
			continue
		}
		if strings.Contains(token, "model_reasoning_effort") {
			if value, ok := ExtractEffort(token); ok {
				detected = value
			}
		}
	}
	if detected == "" {
		return "", false
	}
	return detected, true
}

// ExtractEffort parses a model_reasoning_effort value from a config string.
func ExtractEffort(config string) (string, bool) {
	idx := strings.Index(config, "model_reasoning_effort")
	if idx == -1 {
		return "", false
	}
	parts := strings.SplitN(config[idx:], "=", 2)
	if len(parts) != 2 {
		return "", false
	}
	value := strings.TrimSpace(parts[1])
	value = strings.Trim(value, "\"'")
	value = strings.TrimSpace(value)
	if value == "" {
		return "", false
	}
	return strings.ToLower(value), true
}
