// Package tui provides the configuration editor view and save logic.
// Entrypoint: configEditor.
package tui

import (
	"fmt"
	"strconv"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

type configEditor struct {
	locations paths.Locations
	drafts    map[string]config.PartialConfig
	layer     string
	action    string
	apply     bool
	data      configFormData
	form      *huh.Form
	saveError string
	saveNote  string
	width     int
	height    int
}

type configFormData struct {
	UITheme           string
	RefreshSeconds    string
	LogLevel          string
	LogFile           string
	LogRedactionMode  string
	DataDir           string
	CacheDir          string
	PinDir            string
	RunnerMaxWorkers  string
	RunnerDryRun      bool
	SpecsAutofill     bool
	SpecsRunner       string
	SpecsRunnerArgs   string
	SpecsEffort       string
	LoopWorkers       string
	LoopPollSeconds   string
	LoopSleepSeconds  string
	LoopMaxIterations string
	LoopMaxStalled    string
	LoopMaxRepair     string
	LoopOnlyTags      string
	LoopRequireMain   bool
	LoopRunner        string
	LoopRunnerArgs    string
	LoopEffort        string
	GitAutoCommit     bool
	GitAutoPush       bool
	GitRequireClean   bool
	GitCommitPrefix   string
}

const (
	layerGlobal  = "global"
	layerRepo    = "repo"
	layerSession = "session"
)

const (
	actionSaveGlobal = "save_global"
	actionSaveRepo   = "save_repo"
	actionDiscard    = "discard_session"
)

func newConfigEditor(locations paths.Locations) (*configEditor, error) {
	editor := &configEditor{
		locations: locations,
		drafts:    map[string]config.PartialConfig{},
		layer:     layerRepo,
		action:    actionSaveRepo,
	}

	var globalPartial *config.PartialConfig
	if locations.GlobalConfigPath != "" {
		loaded, err := config.LoadPartial(locations.GlobalConfigPath)
		if err != nil {
			return nil, err
		}
		globalPartial = loaded
	}

	var repoPartial *config.PartialConfig
	if locations.RepoConfigPath != "" {
		loaded, err := config.LoadPartial(locations.RepoConfigPath)
		if err != nil {
			return nil, err
		}
		repoPartial = loaded
	}

	if globalPartial != nil {
		editor.drafts[layerGlobal] = *globalPartial
	}
	if repoPartial != nil {
		editor.drafts[layerRepo] = *repoPartial
	}
	editor.drafts[layerSession] = config.PartialConfig{}

	if err := editor.resetLayer(editor.layer); err != nil {
		return nil, err
	}

	return editor, nil
}

func (e *configEditor) Update(msg tea.Msg) tea.Cmd {
	prevLayer := e.layer
	model, cmd := e.form.Update(msg)
	if form, ok := model.(*huh.Form); ok {
		e.form = form
	}

	if e.layer != prevLayer {
		if err := e.commitDraft(prevLayer); err != nil {
			e.layer = prevLayer
			e.saveError = err.Error()
			e.saveNote = ""
		} else {
			_ = e.resetLayer(e.layer)
		}
	}

	if e.apply {
		e.apply = false
		e.handleAction(e.action)
	}

	return cmd
}

func (e *configEditor) View() string {
	header := fmt.Sprintf("Config (editing: %s)", layerLabel(e.layer))
	status := e.statusLine()
	return withFinalNewline(header + "\n" + status + "\n\n" + e.form.View())
}

func (e *configEditor) Resize(width int, height int) {
	e.width = width
	e.height = height
	if e.form == nil {
		return
	}
	formHeight := height - 3
	if formHeight < 1 {
		formHeight = 1
	}
	if width > 0 {
		e.form = e.form.WithWidth(width)
	}
	if height > 0 {
		e.form = e.form.WithHeight(formHeight)
	}
}

func (e *configEditor) statusLine() string {
	if e.saveError != "" {
		return fmt.Sprintf("Error: %s", e.saveError)
	}
	if e.saveNote != "" {
		return e.saveNote
	}
	return ""
}

func (e *configEditor) SaveGlobal() {
	e.handleAction(actionSaveGlobal)
}

func (e *configEditor) SaveRepo() {
	e.handleAction(actionSaveRepo)
}

func (e *configEditor) DiscardSession() {
	e.handleAction(actionDiscard)
}

func (e *configEditor) handleAction(action string) {
	if action == "" {
		return
	}
	if err := e.commitDraft(e.layer); err != nil {
		e.saveError = err.Error()
		e.saveNote = ""
		return
	}
	switch action {
	case actionSaveGlobal:
		e.saveLayer(layerGlobal)
	case actionSaveRepo:
		e.saveLayer(layerRepo)
	case actionDiscard:
		e.discardSession()
	}
}

func (e *configEditor) saveLayer(layer string) {
	partial, ok := e.drafts[layer]
	if !ok {
		partial = config.PartialConfig{}
	}
	if err := e.validatePartial(layer, partial); err != nil {
		e.saveError = err.Error()
		e.saveNote = ""
		return
	}

	var path string
	var options config.SaveOptions
	if layer == layerGlobal {
		path = e.locations.GlobalConfigPath
		if path == "" {
			e.saveError = "global config path unavailable"
			e.saveNote = ""
			return
		}
	} else if layer == layerRepo {
		path = e.locations.RepoConfigPath
		options.RelativeRoot = e.locations.RepoRoot
	} else {
		e.saveError = "session layer cannot be saved"
		e.saveNote = ""
		return
	}

	if err := config.SavePartial(path, partial, options); err != nil {
		e.saveError = err.Error()
		e.saveNote = ""
		return
	}

	e.saveError = ""
	e.saveNote = fmt.Sprintf("Saved %s config to %s", layer, path)
	_ = e.resetLayer(e.layer)
}

func (e *configEditor) discardSession() {
	e.drafts[layerSession] = config.PartialConfig{}
	e.saveError = ""
	e.saveNote = "Cleared session overrides"
	if e.layer == layerSession {
		_ = e.resetLayer(layerSession)
	}
}

func (e *configEditor) resetLayer(layer string) error {
	cfg, err := e.effectiveConfig(layer)
	if err != nil {
		return err
	}
	e.layer = layer
	if e.layer == layerGlobal {
		e.action = actionSaveGlobal
	} else if e.layer == layerRepo {
		e.action = actionSaveRepo
	} else {
		e.action = actionDiscard
	}
	e.data = formDataFromConfig(cfg)
	e.form = e.buildForm()
	e.Resize(e.width, e.height)
	return nil
}

func (e *configEditor) commitDraft(layer string) error {
	partial, err := partialFromForm(e.data)
	if err != nil {
		return err
	}
	e.drafts[layer] = partial
	return nil
}

func (e *configEditor) effectiveConfig(layer string) (config.Config, error) {
	defaults, err := config.DefaultConfig()
	if err != nil {
		return config.Config{}, err
	}
	defaults = config.ResolvePaths(defaults, e.locations.RepoRoot)

	globalDraft := e.drafts[layerGlobal]
	globalCfg, err := config.ApplyPartial(defaults, globalDraft, e.locations.HomeDir)
	if err != nil {
		return config.Config{}, err
	}

	repoDraft := e.drafts[layerRepo]
	repoCfg, err := config.ApplyPartial(globalCfg, repoDraft, e.locations.RepoRoot)
	if err != nil {
		return config.Config{}, err
	}

	sessionDraft := e.drafts[layerSession]
	sessionCfg, err := config.ApplyPartial(repoCfg, sessionDraft, e.locations.CWD)
	if err != nil {
		return config.Config{}, err
	}

	switch layer {
	case layerGlobal:
		return globalCfg, nil
	case layerRepo:
		return repoCfg, nil
	case layerSession:
		return sessionCfg, nil
	default:
		return repoCfg, nil
	}
}

func (e *configEditor) validatePartial(layer string, partial config.PartialConfig) error {
	defaults, err := config.DefaultConfig()
	if err != nil {
		return err
	}
	defaults = config.ResolvePaths(defaults, e.locations.RepoRoot)

	var base config.Config
	switch layer {
	case layerGlobal:
		base = defaults
	case layerRepo:
		base, err = config.ApplyPartial(defaults, e.drafts[layerGlobal], e.locations.HomeDir)
		if err != nil {
			return err
		}
	case layerSession:
		base, err = config.ApplyPartial(defaults, e.drafts[layerGlobal], e.locations.HomeDir)
		if err != nil {
			return err
		}
		base, err = config.ApplyPartial(base, e.drafts[layerRepo], e.locations.RepoRoot)
		if err != nil {
			return err
		}
	default:
		return fmt.Errorf("unknown layer")
	}

	basePath := e.locations.CWD
	if layer == layerGlobal {
		basePath = e.locations.HomeDir
	} else if layer == layerRepo {
		basePath = e.locations.RepoRoot
	}

	cfg, err := config.ApplyPartial(base, partial, basePath)
	if err != nil {
		return err
	}
	return cfg.Validate()
}

func (e *configEditor) buildForm() *huh.Form {
	return huh.NewForm(
		huh.NewGroup(
			huh.NewSelect[string]().
				Title("Layer").
				Options(
					huh.NewOption("Global", layerGlobal),
					huh.NewOption("Repo", layerRepo),
					huh.NewOption("Session", layerSession),
				).
				Value(&e.layer),
		),
		huh.NewGroup(
			huh.NewInput().Title("UI Theme").Value(&e.data.UITheme).Validate(nonEmptyString("ui.theme")),
			huh.NewInput().Title("Refresh Seconds").Value(&e.data.RefreshSeconds).Validate(positiveInt("ui.refresh_seconds")),
			huh.NewInput().Title("Log Level").Value(&e.data.LogLevel).Validate(logLevelValidator()),
			huh.NewSelect[string]().
				Title("Log Redaction Mode").
				Options(
					huh.NewOption("secrets_only (default)", string(redaction.ModeSecretsOnly)),
					huh.NewOption("all_env", string(redaction.ModeAllEnv)),
					huh.NewOption("off", string(redaction.ModeOff)),
				).
				Value(&e.data.LogRedactionMode),
			huh.NewInput().Title("Log File").Value(&e.data.LogFile),
		),
		huh.NewGroup(
			huh.NewInput().Title("Data Dir").Value(&e.data.DataDir).Validate(nonEmptyString("paths.data_dir")),
			huh.NewInput().Title("Cache Dir").Value(&e.data.CacheDir).Validate(nonEmptyString("paths.cache_dir")),
			huh.NewInput().Title("Pin Dir").Value(&e.data.PinDir).Validate(nonEmptyString("paths.pin_dir")),
		),
		huh.NewGroup(
			huh.NewInput().Title("Runner Max Workers").Value(&e.data.RunnerMaxWorkers).Validate(positiveInt("runner.max_workers")),
			huh.NewConfirm().Title("Runner Dry Run").Value(&e.data.RunnerDryRun),
		),
		huh.NewGroup(
			huh.NewConfirm().Title("Specs Autofill Scout").Value(&e.data.SpecsAutofill),
			huh.NewSelect[string]().
				Title("Specs Runner").
				Options(
					huh.NewOption("codex", "codex"),
					huh.NewOption("opencode", "opencode"),
				).
				Value(&e.data.SpecsRunner),
			huh.NewText().Title("Specs Runner Args (one per line)").Value(&e.data.SpecsRunnerArgs).Lines(3),
			huh.NewSelect[string]().
				Title("Specs Reasoning Effort").
				Options(
					huh.NewOption("auto", "auto"),
					huh.NewOption("low", "low"),
					huh.NewOption("medium", "medium"),
					huh.NewOption("high", "high"),
					huh.NewOption("off", "off"),
				).
				Value(&e.data.SpecsEffort),
		),
		huh.NewGroup(
			huh.NewInput().Title("Loop Workers").Value(&e.data.LoopWorkers).Validate(positiveInt("loop.workers")),
			huh.NewInput().Title("Loop Poll Seconds").Value(&e.data.LoopPollSeconds).Validate(positiveInt("loop.poll_seconds")),
			huh.NewInput().Title("Sleep Seconds").Value(&e.data.LoopSleepSeconds).Validate(nonNegativeInt("loop.sleep_seconds")),
			huh.NewInput().Title("Max Iterations").Value(&e.data.LoopMaxIterations).Validate(nonNegativeInt("loop.max_iterations")),
			huh.NewInput().Title("Max Stalled").Value(&e.data.LoopMaxStalled).Validate(nonNegativeInt("loop.max_stalled")),
			huh.NewInput().Title("Max Repair Attempts").Value(&e.data.LoopMaxRepair).Validate(nonNegativeInt("loop.max_repair_attempts")),
			huh.NewInput().Title("Only Tags").Value(&e.data.LoopOnlyTags),
			huh.NewConfirm().Title("Require Main Branch").Value(&e.data.LoopRequireMain),
			huh.NewSelect[string]().
				Title("Loop Runner").
				Options(
					huh.NewOption("codex", "codex"),
					huh.NewOption("opencode", "opencode"),
				).
				Value(&e.data.LoopRunner),
			huh.NewText().Title("Loop Runner Args (one per line)").Value(&e.data.LoopRunnerArgs).Lines(3),
			huh.NewSelect[string]().
				Title("Loop Reasoning Effort").
				Options(
					huh.NewOption("auto", "auto"),
					huh.NewOption("low", "low"),
					huh.NewOption("medium", "medium"),
					huh.NewOption("high", "high"),
					huh.NewOption("off", "off"),
				).
				Value(&e.data.LoopEffort),
		),
		huh.NewGroup(
			huh.NewConfirm().Title("Git Auto Commit").Value(&e.data.GitAutoCommit),
			huh.NewConfirm().Title("Git Auto Push").Value(&e.data.GitAutoPush),
			huh.NewConfirm().Title("Git Require Clean").Value(&e.data.GitRequireClean),
			huh.NewInput().Title("Git Commit Prefix").Value(&e.data.GitCommitPrefix).Validate(nonEmptyString("git.commit_prefix")),
		),
		huh.NewGroup(
			huh.NewSelect[string]().
				Title("Action").
				Options(
					huh.NewOption("Save to Global", actionSaveGlobal),
					huh.NewOption("Save to Repo", actionSaveRepo),
					huh.NewOption("Discard Session", actionDiscard),
				).
				Value(&e.action),
			huh.NewConfirm().Title("Apply action").Value(&e.apply),
		),
	).WithShowHelp(false)
}

func formDataFromConfig(cfg config.Config) configFormData {
	return configFormData{
		UITheme:           cfg.UI.Theme,
		RefreshSeconds:    strconv.Itoa(cfg.UI.RefreshSeconds),
		LogLevel:          cfg.Logging.Level,
		LogFile:           cfg.Logging.File,
		LogRedactionMode:  string(cfg.Logging.RedactionMode),
		DataDir:           cfg.Paths.DataDir,
		CacheDir:          cfg.Paths.CacheDir,
		PinDir:            cfg.Paths.PinDir,
		RunnerMaxWorkers:  strconv.Itoa(cfg.Runner.MaxWorkers),
		RunnerDryRun:      cfg.Runner.DryRun,
		SpecsAutofill:     cfg.Specs.AutofillScout,
		SpecsRunner:       cfg.Specs.Runner,
		SpecsRunnerArgs:   formatArgsLines(cfg.Specs.RunnerArgs),
		SpecsEffort:       displayReasoningEffort(cfg.Specs.ReasoningEffort),
		LoopWorkers:       strconv.Itoa(cfg.Loop.Workers),
		LoopPollSeconds:   strconv.Itoa(cfg.Loop.PollSeconds),
		LoopSleepSeconds:  strconv.Itoa(cfg.Loop.SleepSeconds),
		LoopMaxIterations: strconv.Itoa(cfg.Loop.MaxIterations),
		LoopMaxStalled:    strconv.Itoa(cfg.Loop.MaxStalled),
		LoopMaxRepair:     strconv.Itoa(cfg.Loop.MaxRepairAttempts),
		LoopOnlyTags:      cfg.Loop.OnlyTags,
		LoopRequireMain:   cfg.Loop.RequireMain,
		LoopRunner:        cfg.Loop.Runner,
		LoopRunnerArgs:    formatArgsLines(cfg.Loop.RunnerArgs),
		LoopEffort:        displayReasoningEffort(cfg.Loop.ReasoningEffort),
		GitAutoCommit:     cfg.Git.AutoCommit,
		GitAutoPush:       cfg.Git.AutoPush,
		GitRequireClean:   cfg.Git.RequireClean,
		GitCommitPrefix:   cfg.Git.CommitPrefix,
	}
}

func partialFromForm(data configFormData) (config.PartialConfig, error) {
	refreshSeconds, err := parsePositiveInt("ui.refresh_seconds", data.RefreshSeconds)
	if err != nil {
		return config.PartialConfig{}, err
	}
	runnerWorkers, err := parsePositiveInt("runner.max_workers", data.RunnerMaxWorkers)
	if err != nil {
		return config.PartialConfig{}, err
	}
	loopWorkers, err := parsePositiveInt("loop.workers", data.LoopWorkers)
	if err != nil {
		return config.PartialConfig{}, err
	}
	pollSeconds, err := parsePositiveInt("loop.poll_seconds", data.LoopPollSeconds)
	if err != nil {
		return config.PartialConfig{}, err
	}
	sleepSeconds, err := parseNonNegativeInt("loop.sleep_seconds", data.LoopSleepSeconds)
	if err != nil {
		return config.PartialConfig{}, err
	}
	maxIterations, err := parseNonNegativeInt("loop.max_iterations", data.LoopMaxIterations)
	if err != nil {
		return config.PartialConfig{}, err
	}
	maxStalled, err := parseNonNegativeInt("loop.max_stalled", data.LoopMaxStalled)
	if err != nil {
		return config.PartialConfig{}, err
	}
	maxRepair, err := parseNonNegativeInt("loop.max_repair_attempts", data.LoopMaxRepair)
	if err != nil {
		return config.PartialConfig{}, err
	}
	logLevel := strings.ToLower(strings.TrimSpace(data.LogLevel))
	if !isValidLogLevel(logLevel) {
		return config.PartialConfig{}, fmt.Errorf("logging.level must be one of debug, info, warn, or error")
	}
	logFile := strings.TrimSpace(data.LogFile)
	logRedactionMode := strings.TrimSpace(data.LogRedactionMode)
	if logRedactionMode == "" {
		logRedactionMode = string(redaction.ModeSecretsOnly)
	}
	if !redaction.ValidMode(logRedactionMode) {
		return config.PartialConfig{}, fmt.Errorf("logging.redaction_mode must be one of off, secrets_only, or all_env")
	}
	specsRunner := strings.TrimSpace(data.SpecsRunner)
	if specsRunner == "" {
		return config.PartialConfig{}, fmt.Errorf("specs.runner must be set")
	}
	if !config.ValidRunner(specsRunner) {
		return config.PartialConfig{}, fmt.Errorf("specs.runner must be codex or opencode")
	}
	loopRunner := strings.TrimSpace(data.LoopRunner)
	if loopRunner == "" {
		return config.PartialConfig{}, fmt.Errorf("loop.runner must be set")
	}
	if !config.ValidRunner(loopRunner) {
		return config.PartialConfig{}, fmt.Errorf("loop.runner must be codex or opencode")
	}
	specsEffort := strings.ToLower(strings.TrimSpace(data.SpecsEffort))
	if specsEffort == "" {
		specsEffort = "auto"
	}
	if !config.ValidReasoningEffort(specsEffort) {
		return config.PartialConfig{}, fmt.Errorf("specs.reasoning_effort must be auto, low, medium, high, or off")
	}
	loopEffort := strings.ToLower(strings.TrimSpace(data.LoopEffort))
	if loopEffort == "" {
		loopEffort = "auto"
	}
	if !config.ValidReasoningEffort(loopEffort) {
		return config.PartialConfig{}, fmt.Errorf("loop.reasoning_effort must be auto, low, medium, high, or off")
	}

	uiTheme := strings.TrimSpace(data.UITheme)
	dataDir := strings.TrimSpace(data.DataDir)
	cacheDir := strings.TrimSpace(data.CacheDir)
	pinDir := strings.TrimSpace(data.PinDir)
	commitPrefix := strings.TrimSpace(data.GitCommitPrefix)
	logMode := redaction.NormalizeMode(logRedactionMode)

	return config.PartialConfig{
		Version: intPtr(1),
		UI: &config.UIPartial{
			Theme:          &uiTheme,
			RefreshSeconds: &refreshSeconds,
		},
		Logging: &config.LoggingPartial{
			Level:         &logLevel,
			File:          &logFile,
			RedactionMode: &logMode,
		},
		Paths: &config.PathsPartial{
			DataDir:  &dataDir,
			CacheDir: &cacheDir,
			PinDir:   &pinDir,
		},
		Runner: &config.RunnerPartial{
			MaxWorkers: &runnerWorkers,
			DryRun:     &data.RunnerDryRun,
		},
		Specs: &config.SpecsPartial{
			AutofillScout:   &data.SpecsAutofill,
			Runner:          &specsRunner,
			RunnerArgs:      parseArgsLines(data.SpecsRunnerArgs),
			ReasoningEffort: &specsEffort,
		},
		Loop: &config.LoopPartial{
			Workers:           &loopWorkers,
			PollSeconds:       &pollSeconds,
			SleepSeconds:      &sleepSeconds,
			MaxIterations:     &maxIterations,
			MaxStalled:        &maxStalled,
			MaxRepairAttempts: &maxRepair,
			OnlyTags:          &data.LoopOnlyTags,
			RequireMain:       &data.LoopRequireMain,
			Runner:            &loopRunner,
			RunnerArgs:        parseArgsLines(data.LoopRunnerArgs),
			ReasoningEffort:   &loopEffort,
		},
		Git: &config.GitPartial{
			AutoCommit:   &data.GitAutoCommit,
			AutoPush:     &data.GitAutoPush,
			RequireClean: &data.GitRequireClean,
			CommitPrefix: &commitPrefix,
		},
	}, nil
}

func intPtr(value int) *int {
	return &value
}

func nonEmptyString(label string) func(string) error {
	return func(value string) error {
		if strings.TrimSpace(value) == "" {
			return fmt.Errorf("%s must be set", label)
		}
		return nil
	}
}

func positiveInt(label string) func(string) error {
	return func(value string) error {
		_, err := parsePositiveInt(label, value)
		return err
	}
}

func nonNegativeInt(label string) func(string) error {
	return func(value string) error {
		_, err := parseNonNegativeInt(label, value)
		return err
	}
}

func parsePositiveInt(label string, value string) (int, error) {
	trimmed := strings.TrimSpace(value)
	parsed, err := strconv.Atoi(trimmed)
	if err != nil || parsed <= 0 {
		return 0, fmt.Errorf("%s must be a positive integer", label)
	}
	return parsed, nil
}

func parseNonNegativeInt(label string, value string) (int, error) {
	trimmed := strings.TrimSpace(value)
	parsed, err := strconv.Atoi(trimmed)
	if err != nil || parsed < 0 {
		return 0, fmt.Errorf("%s must be a non-negative integer", label)
	}
	return parsed, nil
}

func logLevelValidator() func(string) error {
	return func(value string) error {
		trimmed := strings.ToLower(strings.TrimSpace(value))
		if !isValidLogLevel(trimmed) {
			return fmt.Errorf("logging.level must be one of debug, info, warn, or error")
		}
		return nil
	}
}

func isValidLogLevel(level string) bool {
	switch level {
	case "debug", "info", "warn", "error":
		return true
	default:
		return false
	}
}

func layerLabel(layer string) string {
	if layer == "" {
		return ""
	}
	return strings.ToUpper(layer[:1]) + layer[1:]
}
