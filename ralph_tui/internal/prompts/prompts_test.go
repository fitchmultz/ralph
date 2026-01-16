package prompts

import (
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

func TestWorkerPromptsIncludeSafetySections(t *testing.T) {
	sections := []string{
		"PRE-FLIGHT SAFETY (DIRTY REPO)",
		"STOP/CANCEL SEMANTICS",
		"END-OF-TURN CHECKLIST",
	}
	runners := []Runner{RunnerCodex, RunnerOpencode}
	projectTypes := []project.Type{project.TypeCode, project.TypeDocs}

	for _, runner := range runners {
		for _, projectType := range projectTypes {
			content, err := WorkerPrompt(runner, projectType)
			if err != nil {
				t.Fatalf("failed to load worker prompt for %s/%s: %v", runner, projectType, err)
			}
			for _, section := range sections {
				if !strings.Contains(content, section) {
					t.Fatalf("worker prompt for %s/%s missing section %q", runner, projectType, section)
				}
			}
		}
	}
}

func TestSupervisorPromptIncludesRepairPriority(t *testing.T) {
	content, err := SupervisorPrompt(project.TypeCode)
	if err != nil {
		t.Fatalf("failed to load supervisor prompt: %v", err)
	}
	section := "MECHANICAL REPAIR PRIORITY (BEFORE QUARANTINE)"
	if !strings.Contains(content, section) {
		t.Fatalf("supervisor prompt missing section %q", section)
	}
}

func TestDocsWorkerPromptMentionsDocsWorkflow(t *testing.T) {
	content, err := WorkerPrompt(RunnerCodex, project.TypeDocs)
	if err != nil {
		t.Fatalf("failed to load docs worker prompt: %v", err)
	}
	if !strings.Contains(content, "DOCS ITERATION / COMPLETION WORKFLOW") {
		t.Fatalf("expected docs worker prompt to mention docs iteration workflow")
	}
	if !strings.Contains(content, "placeholders") {
		t.Fatalf("expected docs worker prompt to mention placeholders")
	}
	if !strings.Contains(content, "cross-links") {
		t.Fatalf("expected docs worker prompt to mention cross-links")
	}
}
