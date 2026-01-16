package prompts

import (
	"os"
	"path/filepath"
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

func TestEmbeddedDefaultsIncludeRequiredTemplates(t *testing.T) {
	required := []string{
		"defaults/prompt_codex.md",
		"defaults/prompt_codex_docs.md",
		"defaults/prompt_opencode.md",
		"defaults/prompt_opencode_docs.md",
		"defaults/supervisor_prompt.md",
		"defaults/specs_bug_sweep_code.md",
		"defaults/specs_bug_sweep_docs.md",
		"defaults/pin_implementation_queue.md",
		"defaults/pin_implementation_done.md",
		"defaults/pin_lookup_table.md",
		"defaults/pin_readme.md",
		"defaults/pin_specs_builder.md",
		"defaults/pin_specs_builder_docs.md",
		"defaults/specs_interactive_instructions.md",
		"defaults/specs_innovate_instructions_code.md",
		"defaults/specs_innovate_instructions_docs.md",
		"defaults/specs_scout_workflow_template_code.md",
		"defaults/specs_scout_workflow_template_docs.md",
	}

	for _, path := range required {
		if _, err := defaultPrompts.ReadFile(path); err != nil {
			t.Fatalf("missing embedded default %q: %v", path, err)
		}
	}
}

func TestRepoPinTemplatesMatchEmbeddedDefaults(t *testing.T) {
	cases := []struct {
		name        string
		embedded    string
		repoRelPath []string
	}{
		{
			name:        "specs_builder",
			embedded:    pinSpecsBuilderCodePath,
			repoRelPath: []string{".ralph", "pin", "specs_builder.md"},
		},
		{
			name:        "specs_builder_docs",
			embedded:    pinSpecsBuilderDocsPath,
			repoRelPath: []string{".ralph", "pin", "specs_builder_docs.md"},
		},
		{
			name:        "pin_readme",
			embedded:    pinReadmePath,
			repoRelPath: []string{".ralph", "pin", "README.md"},
		},
	}

	for _, tc := range cases {
		embeddedContent, err := readDefault(tc.embedded)
		if err != nil {
			t.Fatalf("failed to read embedded %s: %v", tc.name, err)
		}
		repoContent, err := readRepoFile(tc.repoRelPath...)
		if err != nil {
			t.Fatalf("failed to read repo %s: %v", tc.name, err)
		}
		if strings.TrimSpace(embeddedContent) != strings.TrimSpace(repoContent) {
			t.Fatalf("repo %s does not match embedded template", tc.name)
		}
	}
}

func readRepoFile(parts ...string) (string, error) {
	repoRoot := filepath.Join(append([]string{"..", "..", ".."}, parts...)...)
	content, err := os.ReadFile(repoRoot)
	if err != nil {
		return "", err
	}
	return string(content), nil
}
