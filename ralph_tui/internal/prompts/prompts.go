// Package prompts provides embedded default prompt templates for the loop runner.
package prompts

import (
	"embed"
	"fmt"
)

type Runner string

const (
	RunnerCodex    Runner = "codex"
	RunnerOpencode Runner = "opencode"
)

//go:embed defaults/*
var defaultPrompts embed.FS

// WorkerPrompt returns the default worker prompt content for a runner.
func WorkerPrompt(runner Runner) (string, error) {
	filename := "defaults/prompt_codex.md"
	switch runner {
	case RunnerCodex:
		filename = "defaults/prompt_codex.md"
	case RunnerOpencode:
		filename = "defaults/prompt_opencode.md"
	default:
		return "", fmt.Errorf("unsupported runner: %s", runner)
	}

	content, err := defaultPrompts.ReadFile(filename)
	if err != nil {
		return "", err
	}
	return string(content), nil
}

// SupervisorPrompt returns the default supervisor prompt content.
func SupervisorPrompt() (string, error) {
	content, err := defaultPrompts.ReadFile("defaults/supervisor_prompt.md")
	if err != nil {
		return "", err
	}
	return string(content), nil
}
