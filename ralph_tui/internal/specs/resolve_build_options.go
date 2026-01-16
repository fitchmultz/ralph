// Package specs centralizes build option resolution for specs prompts.
// Entrypoint: ResolveBuildOptions.
package specs

import (
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

// BuildOptionDefaults captures baseline specs build options.
type BuildOptionDefaults struct {
	Innovate      bool
	AutofillScout bool
	ScoutWorkflow bool
	UserFocus     string

	Runner          Runner
	RunnerArgs      []string
	ReasoningEffort string
}

// BuildOptionOverrides captures explicit overrides for build options.
type BuildOptionOverrides struct {
	Innovate      *bool
	AutofillScout *bool
	ScoutWorkflow *bool
	UserFocus     *string

	Runner          *Runner
	ReasoningEffort *string
}

// ResolvedBuildOptions captures the final build options after resolution.
type ResolvedBuildOptions struct {
	Innovate         bool
	InnovateExplicit bool

	AutofillScout bool
	ScoutWorkflow bool
	UserFocus     string

	Runner          Runner
	RunnerArgs      []string
	ReasoningEffort string

	Effort runnerargs.EffortResult
}

// ResolveBuildOptions merges defaults and overrides to produce effective build options.
func ResolveBuildOptions(defaults BuildOptionDefaults, overrides BuildOptionOverrides, extraRunnerArgs []string) ResolvedBuildOptions {
	innovate := defaults.Innovate
	if overrides.Innovate != nil {
		innovate = *overrides.Innovate
	}

	autofillScout := defaults.AutofillScout
	if overrides.AutofillScout != nil {
		autofillScout = *overrides.AutofillScout
	}

	scoutWorkflow := defaults.ScoutWorkflow
	if overrides.ScoutWorkflow != nil {
		scoutWorkflow = *overrides.ScoutWorkflow
	}

	userFocus := defaults.UserFocus
	if overrides.UserFocus != nil {
		userFocus = *overrides.UserFocus
	}
	userFocus = strings.TrimSpace(userFocus)

	runner := defaults.Runner
	if overrides.Runner != nil {
		runner = *overrides.Runner
	}
	runner = normalizeRunner(runner)
	if runner == "" {
		runner = RunnerCodex
	}

	reasoningEffort := defaults.ReasoningEffort
	if overrides.ReasoningEffort != nil {
		reasoningEffort = *overrides.ReasoningEffort
	}
	reasoningEffort = runnerargs.NormalizeEffort(reasoningEffort)

	mergedArgs := runnerargs.MergeArgs(defaults.RunnerArgs, extraRunnerArgs)
	effortResult := runnerargs.ApplyReasoningEffort(string(runner), mergedArgs, reasoningEffort)

	return ResolvedBuildOptions{
		Innovate:         innovate,
		InnovateExplicit: overrides.Innovate != nil,
		AutofillScout:    autofillScout,
		ScoutWorkflow:    scoutWorkflow,
		UserFocus:        userFocus,
		Runner:           runner,
		RunnerArgs:       effortResult.Args,
		ReasoningEffort:  reasoningEffort,
		Effort:           effortResult,
	}
}
