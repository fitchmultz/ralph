// Package config defines Ralph configuration structures and validation helpers.
// Entrypoint: Config.Validate and the PartialConfig override types.
// Notes: This schema intentionally avoids secret-bearing fields.
package config

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

// Config is the fully-resolved configuration used by the app.
type Config struct {
	Version int           `json:"version"`
	UI      UIConfig      `json:"ui"`
	Logging LoggingConfig `json:"logging"`
	Paths   PathsConfig   `json:"paths"`
	Runner  RunnerConfig  `json:"runner"`
	Specs   SpecsConfig   `json:"specs"`
	Loop    LoopConfig    `json:"loop"`
	Git     GitConfig     `json:"git"`
}

// UIConfig controls TUI presentation and refresh behavior.
type UIConfig struct {
	Theme          string `json:"theme"`
	RefreshSeconds int    `json:"refresh_seconds"`
}

// LoggingConfig controls log verbosity.
type LoggingConfig struct {
	Level         string         `json:"level"`
	File          string         `json:"file"`
	RedactionMode redaction.Mode `json:"redaction_mode"`
}

// PathsConfig declares filesystem locations used by Ralph.
// Repo config paths are resolved relative to the repo root when possible.
type PathsConfig struct {
	DataDir  string `json:"data_dir"`
	CacheDir string `json:"cache_dir"`
	PinDir   string `json:"pin_dir"`
}

// RunnerConfig controls worker behavior for the loop runner.
type RunnerConfig struct {
	MaxWorkers int  `json:"max_workers"`
	DryRun     bool `json:"dry_run"`
}

// SpecsConfig controls spec-building features.
type SpecsConfig struct {
	AutofillScout   bool     `json:"autofill_scout"`
	Runner          string   `json:"runner"`
	RunnerArgs      []string `json:"runner_args"`
	ReasoningEffort string   `json:"reasoning_effort"`
}

// LoopConfig controls loop scheduling knobs.
type LoopConfig struct {
	Workers           int      `json:"workers"`
	PollSeconds       int      `json:"poll_seconds"`
	SleepSeconds      int      `json:"sleep_seconds"`
	MaxIterations     int      `json:"max_iterations"`
	MaxStalled        int      `json:"max_stalled"`
	MaxRepairAttempts int      `json:"max_repair_attempts"`
	OnlyTags          string   `json:"only_tags"`
	RequireMain       bool     `json:"require_main"`
	Runner            string   `json:"runner"`
	RunnerArgs        []string `json:"runner_args"`
	ReasoningEffort   string   `json:"reasoning_effort"`
}

// GitConfig controls Git behaviors invoked by Ralph.
type GitConfig struct {
	AutoCommit   bool   `json:"auto_commit"`
	AutoPush     bool   `json:"auto_push"`
	RequireClean bool   `json:"require_clean"`
	CommitPrefix string `json:"commit_prefix"`
}

// Validate enforces invariants for the effective configuration.
func (c Config) Validate() error {
	if c.Version < 1 {
		return fmt.Errorf("version must be >= 1")
	}
	if strings.TrimSpace(c.UI.Theme) == "" {
		return fmt.Errorf("ui.theme must be set")
	}
	if c.UI.RefreshSeconds <= 0 {
		return fmt.Errorf("ui.refresh_seconds must be > 0")
	}
	if !validLogLevel(c.Logging.Level) {
		return fmt.Errorf("logging.level must be one of debug, info, warn, or error")
	}
	if strings.TrimSpace(c.Logging.File) != "" && !filepath.IsAbs(c.Logging.File) {
		return fmt.Errorf("logging.file must be absolute when set")
	}
	if !redaction.ValidMode(string(c.Logging.RedactionMode)) {
		return fmt.Errorf("logging.redaction_mode must be one of off, secrets_only, or all_env")
	}
	if strings.TrimSpace(c.Paths.DataDir) == "" {
		return fmt.Errorf("paths.data_dir must be set")
	}
	if strings.TrimSpace(c.Paths.CacheDir) == "" {
		return fmt.Errorf("paths.cache_dir must be set")
	}
	if strings.TrimSpace(c.Paths.PinDir) == "" {
		return fmt.Errorf("paths.pin_dir must be set")
	}
	if !filepath.IsAbs(c.Paths.DataDir) {
		return fmt.Errorf("paths.data_dir must be absolute")
	}
	if !filepath.IsAbs(c.Paths.CacheDir) {
		return fmt.Errorf("paths.cache_dir must be absolute")
	}
	if !filepath.IsAbs(c.Paths.PinDir) {
		return fmt.Errorf("paths.pin_dir must be absolute")
	}
	if c.Runner.MaxWorkers <= 0 {
		return fmt.Errorf("runner.max_workers must be > 0")
	}
	if strings.TrimSpace(c.Specs.Runner) == "" {
		return fmt.Errorf("specs.runner must be set")
	}
	if !ValidRunner(c.Specs.Runner) {
		return fmt.Errorf("specs.runner must be codex or opencode (got: %s)", c.Specs.Runner)
	}
	if !ValidReasoningEffort(c.Specs.ReasoningEffort) {
		return fmt.Errorf("specs.reasoning_effort must be auto, low, medium, high, or off")
	}
	if c.Loop.Workers <= 0 {
		return fmt.Errorf("loop.workers must be > 0")
	}
	if c.Loop.PollSeconds <= 0 {
		return fmt.Errorf("loop.poll_seconds must be > 0")
	}
	if c.Loop.SleepSeconds < 0 {
		return fmt.Errorf("loop.sleep_seconds must be >= 0")
	}
	if c.Loop.MaxIterations < 0 {
		return fmt.Errorf("loop.max_iterations must be >= 0")
	}
	if c.Loop.MaxStalled < 0 {
		return fmt.Errorf("loop.max_stalled must be >= 0")
	}
	if c.Loop.MaxRepairAttempts < 0 {
		return fmt.Errorf("loop.max_repair_attempts must be >= 0")
	}
	if strings.TrimSpace(c.Loop.Runner) == "" {
		return fmt.Errorf("loop.runner must be set")
	}
	if !ValidRunner(c.Loop.Runner) {
		return fmt.Errorf("loop.runner must be codex or opencode (got: %s)", c.Loop.Runner)
	}
	if !ValidReasoningEffort(c.Loop.ReasoningEffort) {
		return fmt.Errorf("loop.reasoning_effort must be auto, low, medium, high, or off")
	}
	if strings.TrimSpace(c.Git.CommitPrefix) == "" {
		return fmt.Errorf("git.commit_prefix must be set")
	}
	return nil
}

// ValidRunner returns true when the runner value is supported by Ralph.
func ValidRunner(value string) bool {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "codex", "opencode":
		return true
	default:
		return false
	}
}

// ValidReasoningEffort returns true when the reasoning effort value is supported.
func ValidReasoningEffort(value string) bool {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "", "auto", "low", "medium", "high", "off":
		return true
	default:
		return false
	}
}

func validLogLevel(level string) bool {
	switch strings.ToLower(level) {
	case "debug", "info", "warn", "error":
		return true
	default:
		return false
	}
}

// PartialConfig captures optional overrides from config files, flags, or session values.
type PartialConfig struct {
	Version *int            `json:"version,omitempty"`
	UI      *UIPartial      `json:"ui,omitempty"`
	Logging *LoggingPartial `json:"logging,omitempty"`
	Paths   *PathsPartial   `json:"paths,omitempty"`
	Runner  *RunnerPartial  `json:"runner,omitempty"`
	Specs   *SpecsPartial   `json:"specs,omitempty"`
	Loop    *LoopPartial    `json:"loop,omitempty"`
	Git     *GitPartial     `json:"git,omitempty"`
}

// UIPartial overrides UIConfig fields when set.
type UIPartial struct {
	Theme          *string `json:"theme,omitempty"`
	RefreshSeconds *int    `json:"refresh_seconds,omitempty"`
}

// LoggingPartial overrides LoggingConfig fields when set.
type LoggingPartial struct {
	Level         *string         `json:"level,omitempty"`
	File          *string         `json:"file,omitempty"`
	RedactionMode *redaction.Mode `json:"redaction_mode,omitempty"`
}

// PathsPartial overrides PathsConfig fields when set.
type PathsPartial struct {
	DataDir  *string `json:"data_dir,omitempty"`
	CacheDir *string `json:"cache_dir,omitempty"`
	PinDir   *string `json:"pin_dir,omitempty"`
}

// RunnerPartial overrides RunnerConfig fields when set.
type RunnerPartial struct {
	MaxWorkers *int  `json:"max_workers,omitempty"`
	DryRun     *bool `json:"dry_run,omitempty"`
}

// SpecsPartial overrides SpecsConfig fields when set.
type SpecsPartial struct {
	AutofillScout   *bool    `json:"autofill_scout,omitempty"`
	Runner          *string  `json:"runner,omitempty"`
	RunnerArgs      []string `json:"runner_args,omitempty"`
	ReasoningEffort *string  `json:"reasoning_effort,omitempty"`
}

// LoopPartial overrides LoopConfig fields when set.
type LoopPartial struct {
	Workers           *int     `json:"workers,omitempty"`
	PollSeconds       *int     `json:"poll_seconds,omitempty"`
	SleepSeconds      *int     `json:"sleep_seconds,omitempty"`
	MaxIterations     *int     `json:"max_iterations,omitempty"`
	MaxStalled        *int     `json:"max_stalled,omitempty"`
	MaxRepairAttempts *int     `json:"max_repair_attempts,omitempty"`
	OnlyTags          *string  `json:"only_tags,omitempty"`
	RequireMain       *bool    `json:"require_main,omitempty"`
	Runner            *string  `json:"runner,omitempty"`
	RunnerArgs        []string `json:"runner_args,omitempty"`
	ReasoningEffort   *string  `json:"reasoning_effort,omitempty"`
}

// GitPartial overrides GitConfig fields when set.
type GitPartial struct {
	AutoCommit   *bool   `json:"auto_commit,omitempty"`
	AutoPush     *bool   `json:"auto_push,omitempty"`
	RequireClean *bool   `json:"require_clean,omitempty"`
	CommitPrefix *string `json:"commit_prefix,omitempty"`
}
