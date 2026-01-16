// Package main provides tests for CLI helper behavior.
package main

import (
	"fmt"
	"reflect"
	"strings"
	"testing"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
)

func TestResolveFlagString_DefaultFallback(t *testing.T) {
	flags := pflag.NewFlagSet("test", pflag.ContinueOnError)
	flags.String("runner", "codex", "runner")
	if err := flags.Parse([]string{}); err != nil {
		t.Fatalf("parse flags: %v", err)
	}

	value, err := resolveFlagString(flags, "runner", "opencode")
	if err != nil {
		t.Fatalf("resolve flag: %v", err)
	}
	if value != "opencode" {
		t.Fatalf("expected fallback value, got %q", value)
	}
}

func TestResolveFlagString_Changed(t *testing.T) {
	flags := pflag.NewFlagSet("test", pflag.ContinueOnError)
	flags.String("runner", "codex", "runner")
	if err := flags.Parse([]string{"--runner", "opencode"}); err != nil {
		t.Fatalf("parse flags: %v", err)
	}

	value, err := resolveFlagString(flags, "runner", "codex")
	if err != nil {
		t.Fatalf("resolve flag: %v", err)
	}
	if value != "opencode" {
		t.Fatalf("expected flag value, got %q", value)
	}
}

func TestResolveRunnerFlag_NormalizesFallback(t *testing.T) {
	flags := pflag.NewFlagSet("test", pflag.ContinueOnError)
	flags.String("runner", "codex", "runner")
	if err := flags.Parse([]string{}); err != nil {
		t.Fatalf("parse flags: %v", err)
	}

	value, err := resolveRunnerFlag(flags, "runner", " Codex ")
	if err != nil {
		t.Fatalf("resolve runner flag: %v", err)
	}
	if value != "codex" {
		t.Fatalf("expected normalized fallback value, got %q", value)
	}
}

func TestResolveRunnerFlag_NormalizesChanged(t *testing.T) {
	flags := pflag.NewFlagSet("test", pflag.ContinueOnError)
	flags.String("runner", "codex", "runner")
	if err := flags.Parse([]string{"--runner", " OPENcode "}); err != nil {
		t.Fatalf("parse flags: %v", err)
	}

	value, err := resolveRunnerFlag(flags, "runner", "codex")
	if err != nil {
		t.Fatalf("resolve runner flag: %v", err)
	}
	if value != "opencode" {
		t.Fatalf("expected normalized flag value, got %q", value)
	}
}

func TestMergeRunnerArgsWithEffort_ConfigThenCLI(t *testing.T) {
	configArgs := []string{"--agent", "default"}
	cliArgs := []string{"--foo"}

	got := mergeRunnerArgsWithEffort("codex", configArgs, cliArgs, "high")
	want := []string{"-c", "model_reasoning_effort=\"high\"", "--agent", "default", "--foo"}

	if !reflect.DeepEqual(got, want) {
		t.Fatalf("expected args %v, got %v", want, got)
	}
}

func TestMergeRunnerArgsWithEffort_ArgsOverrideEffort(t *testing.T) {
	configArgs := []string{"-c", "model_reasoning_effort=\"low\"", "--agent", "default"}
	cliArgs := []string{"--foo"}

	got := mergeRunnerArgsWithEffort("codex", configArgs, cliArgs, "high")
	want := []string{"-c", "model_reasoning_effort=\"low\"", "--agent", "default", "--foo"}

	if !reflect.DeepEqual(got, want) {
		t.Fatalf("expected args %v, got %v", want, got)
	}
}

func TestParseOnlyTagsCLI(t *testing.T) {
	cases := []struct {
		name  string
		input string
		want  []string
	}{
		{name: "comma-separated", input: "ui,docs", want: []string{"ui", "docs"}},
		{name: "space-separated", input: "ui code", want: []string{"ui", "code"}},
		{name: "brackets-and-mixed", input: "ui, [code] docs", want: []string{"ui", "code", "docs"}},
		{name: "empty", input: " ", want: []string{}},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := parseOnlyTagsCLI(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("expected %v, got %v", tc.want, got)
			}
		})
	}
}

func TestParseOnlyTagsCLI_UnknownTag(t *testing.T) {
	_, err := parseOnlyTagsCLI("ui,unknown")
	if err == nil {
		t.Fatalf("expected error for unknown tag")
	}
	if !strings.Contains(err.Error(), "--only-tag has unsupported tag") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestPinMoveCheckedFlagDefaults(t *testing.T) {
	root := newRootCommand()
	cmd, err := findCommand(root, "pin", "move-checked")
	if err != nil {
		t.Fatalf("locate move-checked: %v", err)
	}

	appendFlag := cmd.Flags().Lookup("append")
	if appendFlag == nil {
		t.Fatalf("expected append flag")
	}
	if appendFlag.DefValue != "false" {
		t.Fatalf("expected append default false, got %q", appendFlag.DefValue)
	}
	if cmd.Flags().Lookup("prepend") != nil {
		t.Fatalf("expected prepend flag removed")
	}
}

func findCommand(root *cobra.Command, path ...string) (*cobra.Command, error) {
	cmd := root
	for _, name := range path {
		found := false
		for _, sub := range cmd.Commands() {
			if sub.Name() == name {
				cmd = sub
				found = true
				break
			}
		}
		if !found {
			return nil, fmt.Errorf("command %q not found", name)
		}
	}
	return cmd, nil
}
