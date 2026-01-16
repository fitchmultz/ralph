// Package prompts provides embedded default prompt templates for the loop runner.
package prompts

import (
	"embed"
	"fmt"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

type Runner string

const (
	RunnerCodex    Runner = "codex"
	RunnerOpencode Runner = "opencode"
)

//go:embed defaults/*
var defaultPrompts embed.FS

const (
	pinQueuePath            = "defaults/pin_implementation_queue.md"
	pinDonePath             = "defaults/pin_implementation_done.md"
	pinLookupPath           = "defaults/pin_lookup_table.md"
	pinReadmePath           = "defaults/pin_readme.md"
	pinSpecsBuilderCodePath = "defaults/pin_specs_builder.md"
	pinSpecsBuilderDocsPath = "defaults/pin_specs_builder_docs.md"

	specsInteractivePath       = "defaults/specs_interactive_instructions.md"
	specsInnovateCodePath      = "defaults/specs_innovate_instructions_code.md"
	specsInnovateDocsPath      = "defaults/specs_innovate_instructions_docs.md"
	specsScoutTemplateCodePath = "defaults/specs_scout_workflow_template_code.md"
	specsScoutTemplateDocsPath = "defaults/specs_scout_workflow_template_docs.md"
	supervisorPromptPath       = "defaults/supervisor_prompt.md"
)

func readDefault(filename string) (string, error) {
	content, err := defaultPrompts.ReadFile(filename)
	if err != nil {
		return "", err
	}
	return string(content), nil
}

// WorkerPrompt returns the default worker prompt content for a runner and project type.
func WorkerPrompt(runner Runner, projectType project.Type) (string, error) {
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", err
	}

	filename, err := workerPromptFilename(runner, resolvedType)
	if err != nil {
		return "", err
	}

	return readDefault(filename)
}

// SupervisorPrompt returns the default supervisor prompt content for a project type.
func SupervisorPrompt(projectType project.Type) (string, error) {
	_, err := project.ResolveType(projectType)
	if err != nil {
		return "", err
	}
	return readDefault(supervisorPromptPath)
}

// PinImplementationQueue returns the default implementation queue content.
func PinImplementationQueue() (string, error) {
	return readDefault(pinQueuePath)
}

// PinImplementationDone returns the default implementation done content.
func PinImplementationDone() (string, error) {
	return readDefault(pinDonePath)
}

// PinLookupTable returns the default lookup table content.
func PinLookupTable() (string, error) {
	return readDefault(pinLookupPath)
}

// PinReadme returns the default pin README content.
func PinReadme() (string, error) {
	return readDefault(pinReadmePath)
}

// PinSpecsBuilder returns the default specs builder template for the project type.
func PinSpecsBuilder(projectType project.Type) (string, error) {
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", err
	}
	path := pinSpecsBuilderCodePath
	if resolvedType == project.TypeDocs {
		path = pinSpecsBuilderDocsPath
	}
	return readDefault(path)
}

// SpecsInteractiveInstructions returns the interactive instructions snippet.
func SpecsInteractiveInstructions() (string, error) {
	return readDefault(specsInteractivePath)
}

// SpecsInnovateInstructions returns the innovate instructions snippet for the project type.
func SpecsInnovateInstructions(projectType project.Type) (string, error) {
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", err
	}
	path := specsInnovateCodePath
	if resolvedType == project.TypeDocs {
		path = specsInnovateDocsPath
	}
	return readDefault(path)
}

// SpecsScoutWorkflowTemplate returns the scout workflow template for the project type.
func SpecsScoutWorkflowTemplate(projectType project.Type) (string, error) {
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", err
	}
	path := specsScoutTemplateCodePath
	if resolvedType == project.TypeDocs {
		path = specsScoutTemplateDocsPath
	}
	return readDefault(path)
}

func workerPromptFilename(runner Runner, projectType project.Type) (string, error) {
	switch runner {
	case RunnerCodex:
		if projectType == project.TypeDocs {
			return "defaults/prompt_codex_docs.md", nil
		}
		return "defaults/prompt_codex.md", nil
	case RunnerOpencode:
		if projectType == project.TypeDocs {
			return "defaults/prompt_opencode_docs.md", nil
		}
		return "defaults/prompt_opencode.md", nil
	default:
		return "", fmt.Errorf("unsupported runner: %s", runner)
	}
}
