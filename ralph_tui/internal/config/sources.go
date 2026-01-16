// Package config reports which configuration layer supplies each effective field.
// Entrypoint: FieldSourcesForConfigs.
package config

import "reflect"

// SourceLayer describes the config layer that supplies an effective value.
type SourceLayer string

const (
	SourceDefault SourceLayer = "default"
	SourceGlobal  SourceLayer = "global"
	SourceRepo    SourceLayer = "repo"
	SourceCLI     SourceLayer = "cli"
	SourceSession SourceLayer = "session"
)

// FieldSources captures the source layer for each configuration field.
type FieldSources struct {
	ProjectType       SourceLayer
	UITheme           SourceLayer
	UIRefreshSeconds  SourceLayer
	LoggingLevel      SourceLayer
	LoggingFile       SourceLayer
	LoggingRedaction  SourceLayer
	LoggingMaxBuffer  SourceLayer
	PathsDataDir      SourceLayer
	PathsCacheDir     SourceLayer
	PathsPinDir       SourceLayer
	SpecsAutofill     SourceLayer
	SpecsScout        SourceLayer
	SpecsUserFocus    SourceLayer
	SpecsRunner       SourceLayer
	SpecsRunnerArgs   SourceLayer
	SpecsEffort       SourceLayer
	LoopSleepSeconds  SourceLayer
	LoopMaxIterations SourceLayer
	LoopMaxStalled    SourceLayer
	LoopMaxRepair     SourceLayer
	LoopInactivity    SourceLayer
	LoopOnlyTags      SourceLayer
	LoopRequireMain   SourceLayer
	LoopRunner        SourceLayer
	LoopRunnerArgs    SourceLayer
	LoopEffort        SourceLayer
	GitAutoCommit     SourceLayer
	GitAutoPush       SourceLayer
}

// FieldSourcesForConfigs compares each layer's config to determine source layers.
func FieldSourcesForConfigs(defaults, globalCfg, repoCfg, cliCfg, sessionCfg Config) FieldSources {
	sources := FieldSources{
		ProjectType:       SourceDefault,
		UITheme:           SourceDefault,
		UIRefreshSeconds:  SourceDefault,
		LoggingLevel:      SourceDefault,
		LoggingFile:       SourceDefault,
		LoggingRedaction:  SourceDefault,
		LoggingMaxBuffer:  SourceDefault,
		PathsDataDir:      SourceDefault,
		PathsCacheDir:     SourceDefault,
		PathsPinDir:       SourceDefault,
		SpecsAutofill:     SourceDefault,
		SpecsScout:        SourceDefault,
		SpecsUserFocus:    SourceDefault,
		SpecsRunner:       SourceDefault,
		SpecsRunnerArgs:   SourceDefault,
		SpecsEffort:       SourceDefault,
		LoopSleepSeconds:  SourceDefault,
		LoopMaxIterations: SourceDefault,
		LoopMaxStalled:    SourceDefault,
		LoopMaxRepair:     SourceDefault,
		LoopInactivity:    SourceDefault,
		LoopOnlyTags:      SourceDefault,
		LoopRequireMain:   SourceDefault,
		LoopRunner:        SourceDefault,
		LoopRunnerArgs:    SourceDefault,
		LoopEffort:        SourceDefault,
		GitAutoCommit:     SourceDefault,
		GitAutoPush:       SourceDefault,
	}

	sources.ProjectType = resolveSource(defaults.ProjectType, globalCfg.ProjectType, repoCfg.ProjectType, cliCfg.ProjectType, sessionCfg.ProjectType)
	sources.UITheme = resolveSource(defaults.UI.Theme, globalCfg.UI.Theme, repoCfg.UI.Theme, cliCfg.UI.Theme, sessionCfg.UI.Theme)
	sources.UIRefreshSeconds = resolveSource(defaults.UI.RefreshSeconds, globalCfg.UI.RefreshSeconds, repoCfg.UI.RefreshSeconds, cliCfg.UI.RefreshSeconds, sessionCfg.UI.RefreshSeconds)
	sources.LoggingLevel = resolveSource(defaults.Logging.Level, globalCfg.Logging.Level, repoCfg.Logging.Level, cliCfg.Logging.Level, sessionCfg.Logging.Level)
	sources.LoggingFile = resolveSource(defaults.Logging.File, globalCfg.Logging.File, repoCfg.Logging.File, cliCfg.Logging.File, sessionCfg.Logging.File)
	sources.LoggingRedaction = resolveSource(defaults.Logging.RedactionMode, globalCfg.Logging.RedactionMode, repoCfg.Logging.RedactionMode, cliCfg.Logging.RedactionMode, sessionCfg.Logging.RedactionMode)
	sources.LoggingMaxBuffer = resolveSource(defaults.Logging.MaxBufferedBytes, globalCfg.Logging.MaxBufferedBytes, repoCfg.Logging.MaxBufferedBytes, cliCfg.Logging.MaxBufferedBytes, sessionCfg.Logging.MaxBufferedBytes)

	sources.PathsDataDir = resolveSource(defaults.Paths.DataDir, globalCfg.Paths.DataDir, repoCfg.Paths.DataDir, cliCfg.Paths.DataDir, sessionCfg.Paths.DataDir)
	sources.PathsCacheDir = resolveSource(defaults.Paths.CacheDir, globalCfg.Paths.CacheDir, repoCfg.Paths.CacheDir, cliCfg.Paths.CacheDir, sessionCfg.Paths.CacheDir)
	sources.PathsPinDir = resolveSource(defaults.Paths.PinDir, globalCfg.Paths.PinDir, repoCfg.Paths.PinDir, cliCfg.Paths.PinDir, sessionCfg.Paths.PinDir)

	sources.SpecsAutofill = resolveSource(defaults.Specs.AutofillScout, globalCfg.Specs.AutofillScout, repoCfg.Specs.AutofillScout, cliCfg.Specs.AutofillScout, sessionCfg.Specs.AutofillScout)
	sources.SpecsScout = resolveSource(defaults.Specs.ScoutWorkflow, globalCfg.Specs.ScoutWorkflow, repoCfg.Specs.ScoutWorkflow, cliCfg.Specs.ScoutWorkflow, sessionCfg.Specs.ScoutWorkflow)
	sources.SpecsUserFocus = resolveSource(defaults.Specs.UserFocus, globalCfg.Specs.UserFocus, repoCfg.Specs.UserFocus, cliCfg.Specs.UserFocus, sessionCfg.Specs.UserFocus)
	sources.SpecsRunner = resolveSource(defaults.Specs.Runner, globalCfg.Specs.Runner, repoCfg.Specs.Runner, cliCfg.Specs.Runner, sessionCfg.Specs.Runner)
	sources.SpecsRunnerArgs = resolveSliceSource(defaults.Specs.RunnerArgs, globalCfg.Specs.RunnerArgs, repoCfg.Specs.RunnerArgs, cliCfg.Specs.RunnerArgs, sessionCfg.Specs.RunnerArgs)
	sources.SpecsEffort = resolveSource(defaults.Specs.ReasoningEffort, globalCfg.Specs.ReasoningEffort, repoCfg.Specs.ReasoningEffort, cliCfg.Specs.ReasoningEffort, sessionCfg.Specs.ReasoningEffort)

	sources.LoopSleepSeconds = resolveSource(defaults.Loop.SleepSeconds, globalCfg.Loop.SleepSeconds, repoCfg.Loop.SleepSeconds, cliCfg.Loop.SleepSeconds, sessionCfg.Loop.SleepSeconds)
	sources.LoopMaxIterations = resolveSource(defaults.Loop.MaxIterations, globalCfg.Loop.MaxIterations, repoCfg.Loop.MaxIterations, cliCfg.Loop.MaxIterations, sessionCfg.Loop.MaxIterations)
	sources.LoopMaxStalled = resolveSource(defaults.Loop.MaxStalled, globalCfg.Loop.MaxStalled, repoCfg.Loop.MaxStalled, cliCfg.Loop.MaxStalled, sessionCfg.Loop.MaxStalled)
	sources.LoopMaxRepair = resolveSource(defaults.Loop.MaxRepairAttempts, globalCfg.Loop.MaxRepairAttempts, repoCfg.Loop.MaxRepairAttempts, cliCfg.Loop.MaxRepairAttempts, sessionCfg.Loop.MaxRepairAttempts)
	sources.LoopInactivity = resolveSource(defaults.Loop.RunnerInactivitySeconds, globalCfg.Loop.RunnerInactivitySeconds, repoCfg.Loop.RunnerInactivitySeconds, cliCfg.Loop.RunnerInactivitySeconds, sessionCfg.Loop.RunnerInactivitySeconds)
	sources.LoopOnlyTags = resolveSource(defaults.Loop.OnlyTags, globalCfg.Loop.OnlyTags, repoCfg.Loop.OnlyTags, cliCfg.Loop.OnlyTags, sessionCfg.Loop.OnlyTags)
	sources.LoopRequireMain = resolveSource(defaults.Loop.RequireMain, globalCfg.Loop.RequireMain, repoCfg.Loop.RequireMain, cliCfg.Loop.RequireMain, sessionCfg.Loop.RequireMain)
	sources.LoopRunner = resolveSource(defaults.Loop.Runner, globalCfg.Loop.Runner, repoCfg.Loop.Runner, cliCfg.Loop.Runner, sessionCfg.Loop.Runner)
	sources.LoopRunnerArgs = resolveSliceSource(defaults.Loop.RunnerArgs, globalCfg.Loop.RunnerArgs, repoCfg.Loop.RunnerArgs, cliCfg.Loop.RunnerArgs, sessionCfg.Loop.RunnerArgs)
	sources.LoopEffort = resolveSource(defaults.Loop.ReasoningEffort, globalCfg.Loop.ReasoningEffort, repoCfg.Loop.ReasoningEffort, cliCfg.Loop.ReasoningEffort, sessionCfg.Loop.ReasoningEffort)

	sources.GitAutoCommit = resolveSource(defaults.Git.AutoCommit, globalCfg.Git.AutoCommit, repoCfg.Git.AutoCommit, cliCfg.Git.AutoCommit, sessionCfg.Git.AutoCommit)
	sources.GitAutoPush = resolveSource(defaults.Git.AutoPush, globalCfg.Git.AutoPush, repoCfg.Git.AutoPush, cliCfg.Git.AutoPush, sessionCfg.Git.AutoPush)

	return sources
}

func resolveSource[T comparable](defaults, globalValue, repoValue, cliValue, sessionValue T) SourceLayer {
	source := SourceDefault
	if globalValue != defaults {
		source = SourceGlobal
	}
	if repoValue != globalValue {
		source = SourceRepo
	}
	if cliValue != repoValue {
		source = SourceCLI
	}
	if sessionValue != cliValue {
		source = SourceSession
	}
	return source
}

func resolveSliceSource[T comparable](defaults, globalValue, repoValue, cliValue, sessionValue []T) SourceLayer {
	source := SourceDefault
	if !reflect.DeepEqual(globalValue, defaults) {
		source = SourceGlobal
	}
	if !reflect.DeepEqual(repoValue, globalValue) {
		source = SourceRepo
	}
	if !reflect.DeepEqual(cliValue, repoValue) {
		source = SourceCLI
	}
	if !reflect.DeepEqual(sessionValue, cliValue) {
		source = SourceSession
	}
	return source
}
