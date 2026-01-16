// Package config tests configuration source attribution helpers.
package config

import (
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

func TestFieldSourcesForConfigs(t *testing.T) {
	defaults, err := DefaultConfig()
	if err != nil {
		t.Fatalf("default config: %v", err)
	}

	globalCfg := defaults
	globalCfg.UI.Theme = "global"
	globalCfg.ProjectType = project.TypeDocs

	repoCfg := globalCfg
	repoCfg.Logging.Level = "warn"
	repoCfg.Specs.RunnerArgs = []string{"-c", "model_reasoning_effort=high"}

	cliCfg := repoCfg
	cliCfg.Loop.OnlyTags = "ui"
	cliCfg.Loop.RunnerInactivitySeconds = 900

	sessionCfg := cliCfg
	sessionCfg.Git.AutoPush = !defaults.Git.AutoPush

	sources := FieldSourcesForConfigs(defaults, globalCfg, repoCfg, cliCfg, sessionCfg)

	if sources.UITheme != SourceGlobal {
		t.Fatalf("expected ui.theme source global, got %q", sources.UITheme)
	}
	if sources.ProjectType != SourceGlobal {
		t.Fatalf("expected project_type source global, got %q", sources.ProjectType)
	}
	if sources.LoggingLevel != SourceRepo {
		t.Fatalf("expected logging.level source repo, got %q", sources.LoggingLevel)
	}
	if sources.SpecsRunnerArgs != SourceRepo {
		t.Fatalf("expected specs.runner_args source repo, got %q", sources.SpecsRunnerArgs)
	}
	if sources.LoopOnlyTags != SourceCLI {
		t.Fatalf("expected loop.only_tags source cli, got %q", sources.LoopOnlyTags)
	}
	if sources.LoopInactivity != SourceCLI {
		t.Fatalf("expected loop.runner_inactivity_seconds source cli, got %q", sources.LoopInactivity)
	}
	if sources.GitAutoPush != SourceSession {
		t.Fatalf("expected git.auto_push source session, got %q", sources.GitAutoPush)
	}
}
