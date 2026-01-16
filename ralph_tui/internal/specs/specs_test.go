// Package specs provides tests for prompt building and innovate resolution.
// Entrypoint: go test ./...
package specs

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

func TestFillPromptReplacements(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "specs_builder.md")
	content := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\n" + scoutPlaceholder
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write template: %v", err)
	}

	prompt, err := FillPrompt(path, FillPromptOptions{
		Interactive:   true,
		Innovate:      true,
		ScoutWorkflow: true,
		UserFocus:     "Focus on specs + tui overlap",
	})
	if err != nil {
		t.Fatalf("FillPrompt failed: %v", err)
	}
	if !strings.Contains(prompt, "INTERACTIVE MODE ENABLED") {
		t.Fatalf("interactive instructions missing")
	}
	if !strings.Contains(prompt, "AUTOFILL/SCOUT MODE ENABLED") {
		t.Fatalf("innovate instructions missing")
	}
	if !strings.Contains(prompt, "SCOUT WORKFLOW ENABLED") {
		t.Fatalf("scout workflow instructions missing")
	}
	if !strings.Contains(prompt, "Focus on specs + tui overlap") {
		t.Fatalf("user focus missing")
	}
}

func TestFillPromptMissingPlaceholderErrors(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "specs_builder.md")
	content := "AGENTS.md\n"
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write template: %v", err)
	}

	_, err := FillPrompt(path, FillPromptOptions{
		Interactive: true,
	})
	if err == nil {
		t.Fatalf("expected error for missing interactive placeholder")
	}
}

func TestFillPromptMissingScoutPlaceholderErrors(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "specs_builder.md")
	content := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write template: %v", err)
	}

	_, err := FillPrompt(path, FillPromptOptions{
		ScoutWorkflow: true,
		UserFocus:     "Pin/specs",
	})
	if err == nil {
		t.Fatalf("expected error for missing scout placeholder")
	}
}

func TestFillPromptBugSweepEntryReplacement(t *testing.T) {
	cases := []struct {
		name        string
		projectType project.Type
		expectType  string
	}{
		{name: "code", projectType: project.TypeCode, expectType: "PROJECT TYPE: CODE"},
		{name: "docs", projectType: project.TypeDocs, expectType: "PROJECT TYPE: DOCS"},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			tmpDir := t.TempDir()
			path := filepath.Join(tmpDir, "specs_builder.md")
			content := "AGENTS.md\n" + bugSweepPlaceholder + "\n"
			if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
				t.Fatalf("write template: %v", err)
			}

			prompt, err := FillPrompt(path, FillPromptOptions{
				ProjectType: testCase.projectType,
			})
			if err != nil {
				t.Fatalf("FillPrompt failed: %v", err)
			}
			if strings.Contains(prompt, bugSweepPlaceholder) {
				t.Fatalf("expected bug sweep placeholder to be replaced")
			}
			if !strings.Contains(prompt, "BUG SWEEP PROMPT ENTRY") {
				t.Fatalf("expected bug sweep entry to be inserted")
			}
			if !strings.Contains(prompt, testCase.expectType) {
				t.Fatalf("expected project type marker %q", testCase.expectType)
			}
		})
	}
}

func TestResolveInnovateAutoEnable(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	queueContent := "## Queue\n\n## Blocked\n\n## Parking Lot\n"
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	effective, err := ResolveInnovate(queuePath, false, false, true)
	if err != nil {
		t.Fatalf("ResolveInnovate failed: %v", err)
	}
	if !effective {
		t.Fatalf("expected innovate auto-enabled when queue empty")
	}
}

func TestResolveInnovateDetailsEmptyQueueReason(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	queueContent := "## Queue\n\n## Blocked\n\n## Parking Lot\n"
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	resolution, err := ResolveInnovateDetails(queuePath, false, false, true)
	if err != nil {
		t.Fatalf("ResolveInnovateDetails failed: %v", err)
	}
	if !resolution.Effective || !resolution.AutoEnabled {
		t.Fatalf("expected auto-enabled innovate for empty queue")
	}
	if resolution.AutoReason != "empty queue" {
		t.Fatalf("expected auto reason to be empty queue, got %q", resolution.AutoReason)
	}
}

func TestResolveInnovateDetailsMissingQueue(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")

	resolution, err := ResolveInnovateDetails(queuePath, false, false, true)
	if err != nil {
		t.Fatalf("ResolveInnovateDetails failed: %v", err)
	}
	if !resolution.Effective || !resolution.AutoEnabled {
		t.Fatalf("expected auto-enabled innovate for missing queue")
	}
	if resolution.AutoReason != "missing queue file" {
		t.Fatalf("expected auto reason to be missing queue file, got %q", resolution.AutoReason)
	}
}

func TestResolveInnovateRespectsExplicit(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	queueContent := "## Queue\n\n## Blocked\n\n## Parking Lot\n"
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	resolution, err := ResolveInnovateDetails(queuePath, false, true, true)
	if err != nil {
		t.Fatalf("ResolveInnovateDetails failed: %v", err)
	}
	if resolution.Effective {
		t.Fatalf("expected explicit innovate false to remain false")
	}
	if resolution.AutoEnabled {
		t.Fatalf("expected auto disabled when innovate explicit")
	}
}

func TestResolveInnovateDetailsNonEmptyQueue(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	queueContent := "## Queue\n- [ ] RQ-0001 [ui]: Item\n\n## Blocked\n\n## Parking Lot\n"
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	resolution, err := ResolveInnovateDetails(queuePath, false, false, true)
	if err != nil {
		t.Fatalf("ResolveInnovateDetails failed: %v", err)
	}
	if resolution.Effective {
		t.Fatalf("expected innovate to remain false when queue has items")
	}
	if resolution.AutoEnabled {
		t.Fatalf("expected auto disabled when queue has items")
	}
	if resolution.AutoReason != "" {
		t.Fatalf("expected empty auto reason, got %q", resolution.AutoReason)
	}
}

func TestResolveBuildOptionsPrecedence(t *testing.T) {
	trueVal := true
	falseVal := false
	focus := "  hello \n"

	resolved := ResolveBuildOptions(
		BuildOptionDefaults{
			Innovate:        false,
			AutofillScout:   false,
			ScoutWorkflow:   false,
			UserFocus:       "default",
			Runner:          Runner(" CODEX "),
			RunnerArgs:      []string{"--base"},
			ReasoningEffort: "Medium",
		},
		BuildOptionOverrides{
			Innovate:      &falseVal,
			AutofillScout: &trueVal,
			ScoutWorkflow: &trueVal,
			UserFocus:     &focus,
		},
		[]string{"--extra"},
	)

	if !resolved.InnovateExplicit || resolved.Innovate {
		t.Fatalf("expected explicit innovate false")
	}
	if !resolved.AutofillScout {
		t.Fatalf("expected autofill override true")
	}
	if !resolved.ScoutWorkflow {
		t.Fatalf("expected scout workflow override true")
	}
	if resolved.UserFocus != "hello" {
		t.Fatalf("expected trimmed focus, got %q", resolved.UserFocus)
	}
	if resolved.Runner != RunnerCodex {
		t.Fatalf("expected runner normalized to codex, got %q", resolved.Runner)
	}
	expectedArgs := []string{"-c", "model_reasoning_effort=\"medium\"", "--base", "--extra"}
	if !reflect.DeepEqual(resolved.RunnerArgs, expectedArgs) {
		t.Fatalf("unexpected runner args: %#v", resolved.RunnerArgs)
	}
	if resolved.Effort.Source != runnerargs.EffortSourceExplicit {
		t.Fatalf("expected explicit effort source, got %s", resolved.Effort.Source)
	}
}

func TestResolveBuildOptionsExistingEffortArg(t *testing.T) {
	resolved := ResolveBuildOptions(
		BuildOptionDefaults{
			Runner:          RunnerCodex,
			RunnerArgs:      []string{"-c", "model_reasoning_effort=\"low\"", "--x"},
			ReasoningEffort: "high",
		},
		BuildOptionOverrides{},
		nil,
	)

	if resolved.Effort.Source != runnerargs.EffortSourceArgs {
		t.Fatalf("expected args effort source, got %s", resolved.Effort.Source)
	}
	if resolved.Effort.Effective != "low" {
		t.Fatalf("expected effective low, got %q", resolved.Effort.Effective)
	}
	if !reflect.DeepEqual(resolved.RunnerArgs, []string{"-c", "model_reasoning_effort=\"low\"", "--x"}) {
		t.Fatalf("unexpected runner args: %#v", resolved.RunnerArgs)
	}
}

func TestResolveBuildOptionsOpencodeNoEffortInjection(t *testing.T) {
	resolved := ResolveBuildOptions(
		BuildOptionDefaults{
			Runner:          RunnerOpencode,
			RunnerArgs:      []string{"--a"},
			ReasoningEffort: "high",
		},
		BuildOptionOverrides{},
		[]string{"--b"},
	)

	if resolved.Effort.Source != runnerargs.EffortSourceNone {
		t.Fatalf("expected no effort source, got %s", resolved.Effort.Source)
	}
	if !reflect.DeepEqual(resolved.RunnerArgs, []string{"--a", "--b"}) {
		t.Fatalf("unexpected runner args: %#v", resolved.RunnerArgs)
	}
}
