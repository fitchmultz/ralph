// Package config defines Ralph configuration structures and validation helpers.
// Entrypoint: Config.Validate and the PartialConfig override types.
// Notes: This schema intentionally avoids secret-bearing fields.
package config

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

// Config is the fully-resolved configuration used by the app.
type Config struct {
	Version     int           `json:"version"`
	ProjectType project.Type  `json:"project_type"`
	UI          UIConfig      `json:"ui"`
	Logging     LoggingConfig `json:"logging"`
	Paths       PathsConfig   `json:"paths"`
	Specs       SpecsConfig   `json:"specs"`
	Loop        LoopConfig    `json:"loop"`
	Git         GitConfig     `json:"git"`
}

// UIConfig controls TUI presentation and refresh behavior.
type UIConfig struct {
	Theme          string `json:"theme"`
	RefreshSeconds int    `json:"refresh_seconds"`
}

// LoggingConfig controls log verbosity.
type LoggingConfig struct {
	Level            string         `json:"level"`
	File             string         `json:"file"`
	RedactionMode    redaction.Mode `json:"redaction_mode"`
	MaxBufferedBytes int            `json:"max_buffered_bytes"`
}

// PathsConfig declares filesystem locations used by Ralph.
// Repo config paths are resolved relative to the repo root when possible.
type PathsConfig struct {
	DataDir            string `json:"data_dir"`
	CacheDir           string `json:"cache_dir"`
	PinDir             string `json:"pin_dir"`
	DoneRetentionLimit int    `json:"done_retention_limit"`
}

// SpecsConfig controls spec-building features.
type SpecsConfig struct {
	AutofillScout   bool     `json:"autofill_scout"`
	ScoutWorkflow   bool     `json:"scout_workflow"`
	UserFocus       string   `json:"user_focus"`
	Runner          string   `json:"runner"`
	RunnerArgs      []string `json:"runner_args"`
	ReasoningEffort string   `json:"reasoning_effort"`
}

// DirtyRepoConfig controls how the loop responds to dirty working trees.
type DirtyRepoConfig struct {
	StartPolicy              string `json:"start_policy"`
	DuringPolicy             string `json:"during_policy"`
	AllowUntracked           bool   `json:"allow_untracked"`
	QuarantineCleanUntracked bool   `json:"quarantine_clean_untracked"`
}

// LoopConfig controls loop scheduling knobs.
type LoopConfig struct {
	SleepSeconds            int             `json:"sleep_seconds"`
	MaxIterations           int             `json:"max_iterations"`
	MaxStalled              int             `json:"max_stalled"`
	MaxRepairAttempts       int             `json:"max_repair_attempts"`
	RunnerInactivitySeconds int             `json:"runner_inactivity_seconds"`
	OnlyTags                string          `json:"only_tags"`
	RequireMain             bool            `json:"require_main"`
	Runner                  string          `json:"runner"`
	RunnerArgs              []string        `json:"runner_args"`
	ReasoningEffort         string          `json:"reasoning_effort"`
	DirtyRepo               DirtyRepoConfig `json:"dirty_repo"`
}

// GitConfig controls Git behaviors invoked by Ralph.
type GitConfig struct {
	AutoCommit bool `json:"auto_commit"`
	AutoPush   bool `json:"auto_push"`
}

// Validate enforces invariants for the effective configuration.
func (c Config) Validate() error {
	if c.Version < 1 {
		return fmt.Errorf("version must be >= 1")
	}
	if !project.ValidType(c.ProjectType) {
		return fmt.Errorf("project_type must be code or docs")
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
	if c.Logging.MaxBufferedBytes < 0 {
		return fmt.Errorf("logging.max_buffered_bytes must be >= 0")
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
	if c.Paths.DoneRetentionLimit < 0 {
		return fmt.Errorf("paths.done_retention_limit must be >= 0")
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
	if c.Loop.RunnerInactivitySeconds < 0 {
		return fmt.Errorf("loop.runner_inactivity_seconds must be >= 0")
	}
	if strings.TrimSpace(c.Loop.OnlyTags) != "" {
		if _, err := pin.ValidateTagList("loop.only_tags", c.Loop.OnlyTags); err != nil {
			return err
		}
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
	if !validDirtyRepoPolicy(c.Loop.DirtyRepo.StartPolicy) {
		return fmt.Errorf("loop.dirty_repo.start_policy must be error, warn, or quarantine")
	}
	if !validDirtyRepoPolicy(c.Loop.DirtyRepo.DuringPolicy) {
		return fmt.Errorf("loop.dirty_repo.during_policy must be error, warn, or quarantine")
	}
	return nil
}

// ValidRunner returns true when the runner value is supported by Ralph.
func ValidRunner(value string) bool {
	switch runnerargs.NormalizeRunner(value) {
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

func validDirtyRepoPolicy(value string) bool {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "error", "warn", "quarantine":
		return true
	default:
		return false
	}
}

// PartialConfig captures optional overrides from config files, flags, or session values.
type PartialConfig struct {
	Version     *int            `json:"version,omitempty"`
	ProjectType *project.Type   `json:"project_type,omitempty"`
	UI          *UIPartial      `json:"ui,omitempty"`
	Logging     *LoggingPartial `json:"logging,omitempty"`
	Paths       *PathsPartial   `json:"paths,omitempty"`
	Specs       *SpecsPartial   `json:"specs,omitempty"`
	Loop        *LoopPartial    `json:"loop,omitempty"`
	Git         *GitPartial     `json:"git,omitempty"`
}

// UIPartial overrides UIConfig fields when set.
type UIPartial struct {
	Theme          *string `json:"theme,omitempty"`
	RefreshSeconds *int    `json:"refresh_seconds,omitempty"`
}

// LoggingPartial overrides LoggingConfig fields when set.
type LoggingPartial struct {
	Level            *string         `json:"level,omitempty"`
	File             *string         `json:"file,omitempty"`
	RedactionMode    *redaction.Mode `json:"redaction_mode,omitempty"`
	MaxBufferedBytes *int            `json:"max_buffered_bytes,omitempty"`
}

// PathsPartial overrides PathsConfig fields when set.
type PathsPartial struct {
	DataDir            *string `json:"data_dir,omitempty"`
	CacheDir           *string `json:"cache_dir,omitempty"`
	PinDir             *string `json:"pin_dir,omitempty"`
	DoneRetentionLimit *int    `json:"done_retention_limit,omitempty"`
}

// SpecsPartial overrides SpecsConfig fields when set.
type SpecsPartial struct {
	AutofillScout   *bool    `json:"autofill_scout,omitempty"`
	ScoutWorkflow   *bool    `json:"scout_workflow,omitempty"`
	UserFocus       *string  `json:"user_focus,omitempty"`
	Runner          *string  `json:"runner,omitempty"`
	RunnerArgs      []string `json:"runner_args,omitempty"`
	ReasoningEffort *string  `json:"reasoning_effort,omitempty"`
}

// DirtyRepoPartial overrides DirtyRepoConfig fields when set.
type DirtyRepoPartial struct {
	StartPolicy              *string `json:"start_policy,omitempty"`
	DuringPolicy             *string `json:"during_policy,omitempty"`
	AllowUntracked           *bool   `json:"allow_untracked,omitempty"`
	QuarantineCleanUntracked *bool   `json:"quarantine_clean_untracked,omitempty"`
}

// LoopPartial overrides LoopConfig fields when set.
type LoopPartial struct {
	SleepSeconds            *int              `json:"sleep_seconds,omitempty"`
	MaxIterations           *int              `json:"max_iterations,omitempty"`
	MaxStalled              *int              `json:"max_stalled,omitempty"`
	MaxRepairAttempts       *int              `json:"max_repair_attempts,omitempty"`
	RunnerInactivitySeconds *int              `json:"runner_inactivity_seconds,omitempty"`
	OnlyTags                *string           `json:"only_tags,omitempty"`
	RequireMain             *bool             `json:"require_main,omitempty"`
	Runner                  *string           `json:"runner,omitempty"`
	RunnerArgs              []string          `json:"runner_args,omitempty"`
	ReasoningEffort         *string           `json:"reasoning_effort,omitempty"`
	DirtyRepo               *DirtyRepoPartial `json:"dirty_repo,omitempty"`
}

// GitPartial overrides GitConfig fields when set.
type GitPartial struct {
	AutoCommit *bool `json:"auto_commit,omitempty"`
	AutoPush   *bool `json:"auto_push,omitempty"`
}
