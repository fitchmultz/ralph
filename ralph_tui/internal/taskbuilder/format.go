package taskbuilder

import (
	"fmt"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/prompts"
)

// FormatOptions controls queue item formatting.
type FormatOptions struct {
	ID          string
	Tags        []string
	Description string
	Scope       string
	Prompt      string
}

// FormatQueueItemBlock returns a block that satisfies pin queue validation rules.
func FormatQueueItemBlock(opts FormatOptions) ([]string, error) {
	if strings.TrimSpace(opts.ID) == "" {
		return nil, fmt.Errorf("queue ID required")
	}
	if strings.TrimSpace(opts.Description) == "" {
		return nil, fmt.Errorf("queue description required")
	}
	tags := normalizeTags(opts.Tags)
	if len(tags) == 0 {
		return nil, fmt.Errorf("at least one routing tag required")
	}
	scope := normalizeScope(opts.Scope)

	header := fmt.Sprintf("- [ ] %s %s: %s %s", opts.ID, formatTags(tags), opts.Description, scope)

	evidenceTemplate, err := prompts.TaskBuilderEvidenceTemplate()
	if err != nil {
		return nil, err
	}
	planTemplate, err := prompts.TaskBuilderPlanTemplate()
	if err != nil {
		return nil, err
	}

	replacements := map[string]string{
		"PROMPT": formatPromptForEvidence(opts.Prompt),
		"SCOPE":  strings.Trim(scope, "()"),
	}

	evidenceLines, err := expandTemplateLines(evidenceTemplate, replacements)
	if err != nil {
		return nil, err
	}
	planLines, err := expandTemplateLines(planTemplate, replacements)
	if err != nil {
		return nil, err
	}

	lines := []string{header}
	lines = append(lines, "  - Evidence:")
	lines = appendIndentedLines(lines, evidenceLines)
	lines = append(lines, "  - Plan:")
	lines = appendIndentedLines(lines, planLines)
	return lines, nil
}

func normalizeScope(scope string) string {
	trimmed := strings.TrimSpace(scope)
	if trimmed == "" {
		trimmed = "repo"
	}
	trimmed = strings.TrimSpace(strings.Trim(trimmed, "()"))
	return fmt.Sprintf("(%s)", trimmed)
}

func normalizeTags(tags []string) []string {
	normalized := make([]string, 0, len(tags))
	seen := make(map[string]struct{}, len(tags))
	for _, tag := range tags {
		value := strings.ToLower(strings.TrimSpace(tag))
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	return normalized
}

func formatTags(tags []string) string {
	parts := make([]string, 0, len(tags))
	for _, tag := range tags {
		parts = append(parts, fmt.Sprintf("[%s]", tag))
	}
	return strings.Join(parts, " ")
}

func appendIndentedLines(lines []string, extra []string) []string {
	for _, line := range extra {
		if strings.TrimSpace(line) == "" {
			continue
		}
		lines = append(lines, "    - "+line)
	}
	return lines
}

// formatPromptForEvidence formats the prompt to be safe and readable in a bullet list.
// It preserves the full text (no truncation) and adds quote markers to multi-line inputs.
func formatPromptForEvidence(prompt string) string {
	trimmed := strings.TrimSpace(prompt)
	if trimmed == "" {
		return "n/a"
	}

	if !strings.Contains(trimmed, "\n") && len(trimmed) < 100 {
		return trimmed
	}

	lines := strings.Split(trimmed, "\n")
	quoted := make([]string, 0, len(lines))
	for _, line := range lines {
		quoted = append(quoted, "> "+line)
	}
	return strings.Join(quoted, "\n")
}

func deriveDescription(prompt string) string {
	trimmed := strings.TrimSpace(prompt)
	if trimmed == "" {
		return ""
	}
	lines := strings.Split(trimmed, "\n")
	first := strings.TrimSpace(lines[0])
	if first == "" {
		first = strings.TrimSpace(strings.Join(lines, " "))
	}
	if first == "" {
		return ""
	}
	return truncateRunes(first, 90)
}

func truncateRunes(value string, max int) string {
	if max <= 0 || value == "" {
		return value
	}
	runes := []rune(value)
	if len(runes) <= max {
		return value
	}
	if max <= 1 {
		return string(runes[:max])
	}
	return string(runes[:max-1]) + "..."
}

func expandTemplateLines(template string, replacements map[string]string) ([]string, error) {
	resolved := template
	for key, value := range replacements {
		resolved = strings.ReplaceAll(resolved, "{{"+key+"}}", value)
	}
	if strings.Contains(resolved, "{{") {
		return nil, fmt.Errorf("unresolved template placeholders in task builder template")
	}
	lines := strings.Split(resolved, "\n")
	output := make([]string, 0, len(lines))
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "- ") {
			trimmed = strings.TrimSpace(strings.TrimPrefix(trimmed, "- "))
		}
		if trimmed == "" {
			continue
		}
		output = append(output, trimmed)
	}
	return output, nil
}
