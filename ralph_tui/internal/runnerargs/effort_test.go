// Package runnerargs provides tests for reasoning effort helpers.
// Entrypoint: go test ./...
package runnerargs

import (
	"reflect"
	"testing"
)

func TestApplyReasoningEffort_ExplicitInjection(t *testing.T) {
	result := ApplyReasoningEffort("codex", []string{"--foo"}, "high")
	wantArgs := []string{"-c", "model_reasoning_effort=\"high\"", "--foo"}
	if !reflect.DeepEqual(result.Args, wantArgs) {
		t.Fatalf("expected args %v, got %v", wantArgs, result.Args)
	}
	if result.Effective != "high" || result.Source != EffortSourceExplicit {
		t.Fatalf("unexpected result: %+v", result)
	}
}

func TestApplyReasoningEffort_ArgsOverride(t *testing.T) {
	args := []string{"-c", "model_reasoning_effort=\"low\"", "--bar"}
	result := ApplyReasoningEffort("codex", args, "high")
	if !reflect.DeepEqual(result.Args, args) {
		t.Fatalf("expected args to be unchanged")
	}
	if result.Effective != "low" || result.Source != EffortSourceArgs {
		t.Fatalf("unexpected result: %+v", result)
	}
}

func TestApplyReasoningEffort_AutoNoInjection(t *testing.T) {
	args := []string{"--baz"}
	result := ApplyReasoningEffort("codex", args, "auto")
	if !reflect.DeepEqual(result.Args, args) {
		t.Fatalf("expected args to be unchanged")
	}
	if result.Effective != "auto" || result.Source != EffortSourceAuto {
		t.Fatalf("unexpected result: %+v", result)
	}
}

func TestApplyReasoningEffort_AutoTargetInjection(t *testing.T) {
	args := []string{"--baz"}
	result := ApplyReasoningEffortWithAutoTarget("codex", args, "auto", "high")
	wantArgs := []string{"-c", "model_reasoning_effort=\"high\"", "--baz"}
	if !reflect.DeepEqual(result.Args, wantArgs) {
		t.Fatalf("expected args %v, got %v", wantArgs, result.Args)
	}
	if result.Effective != "high" || result.Source != EffortSourceAuto {
		t.Fatalf("unexpected result: %+v", result)
	}
}

func TestApplyReasoningEffort_NonCodex(t *testing.T) {
	args := []string{"--baz"}
	result := ApplyReasoningEffort("opencode", args, "high")
	if !reflect.DeepEqual(result.Args, args) {
		t.Fatalf("expected args to be unchanged")
	}
	if result.Effective != "" || result.Source != EffortSourceNone {
		t.Fatalf("unexpected result: %+v", result)
	}
	if got := DisplayEffortResult(result); got != "n/a" {
		t.Fatalf("expected display n/a, got %q", got)
	}
}

func TestSupportsReasoningEffort(t *testing.T) {
	if !SupportsReasoningEffort("codex") {
		t.Fatalf("expected codex to support reasoning effort")
	}
	if SupportsReasoningEffort("opencode") {
		t.Fatalf("expected opencode to not support reasoning effort")
	}
}

func TestDetectEffort_LastValueWins(t *testing.T) {
	args := []string{"-c", "model_reasoning_effort=\"low\"", "model_reasoning_effort=\"high\""}
	value, ok := DetectEffort(args)
	if !ok {
		t.Fatalf("expected effort detection")
	}
	if value != "high" {
		t.Fatalf("expected last effort to win, got %q", value)
	}
}
