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
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

type configEditor struct {
	locations    paths.Locations
	drafts       map[string]config.PartialConfig
	layer        string
	cliOverrides config.PartialConfig
	data         configFormData
	form         *huh.Form
	saveError    string
	saveNote     string
	width        int
	height       int
	fieldDescs   map[string]func(string)
	sources      config.FieldSources
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
	SpecsAutofill     bool
	SpecsScout        bool
	SpecsUserFocus    string
	SpecsRunner       string
	SpecsRunnerArgs   string
	SpecsEffort       string
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

const (
	fieldLayer              = "layer"
	fieldUITheme            = "ui.theme"
	fieldUIRefreshSeconds   = "ui.refresh_seconds"
	fieldLogLevel           = "logging.level"
	fieldLogRedactionMode   = "logging.redaction_mode"
	fieldLogFile            = "logging.file"
	fieldDataDir            = "paths.data_dir"
	fieldCacheDir           = "paths.cache_dir"
	fieldPinDir             = "paths.pin_dir"
	fieldSpecsAutofillScout = "specs.autofill_scout"
	fieldSpecsScoutWorkflow = "specs.scout_workflow"
	fieldSpecsUserFocus     = "specs.user_focus"
	fieldSpecsRunner        = "specs.runner"
	fieldSpecsRunnerArgs    = "specs.runner_args"
	fieldSpecsEffort        = "specs.reasoning_effort"
	fieldLoopSleepSeconds   = "loop.sleep_seconds"
	fieldLoopMaxIterations  = "loop.max_iterations"
	fieldLoopMaxStalled     = "loop.max_stalled"
	fieldLoopMaxRepair      = "loop.max_repair_attempts"
	fieldLoopOnlyTags       = "loop.only_tags"
	fieldLoopRequireMain    = "loop.require_main"
	fieldLoopRunner         = "loop.runner"
	fieldLoopRunnerArgs     = "loop.runner_args"
	fieldLoopEffort         = "loop.reasoning_effort"
	fieldGitAutoCommit      = "git.auto_commit"
	fieldGitAutoPush        = "git.auto_push"
)

func newConfigEditor(
	locations paths.Locations,
	cliOverrides config.PartialConfig,
	sessionOverrides config.PartialConfig,
) (*configEditor, error) {
	editor := &configEditor{
		locations:    locations,
		drafts:       map[string]config.PartialConfig{},
		layer:        layerRepo,
		cliOverrides: cliOverrides,
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
	editor.drafts[layerSession] = sessionOverrides

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

	e.refreshFieldSources()

	return cmd
}

func (e *configEditor) HandlesTabNavigation() bool {
	return e.form != nil
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
	// Ensure the form has built its view even before it receives user input.
	if width > 0 || height > 0 {
		model, _ := e.form.Update(tea.WindowSizeMsg{Width: width, Height: formHeight})
		if form, ok := model.(*huh.Form); ok {
			e.form = form
		}
	}
}

func (e *configEditor) statusLine() string {
	if e.saveError != "" {
		return fmt.Sprintf("Error: %s", e.saveError)
	}
	if e.saveNote != "" {
		return e.saveNote
	}
	return "Save: ctrl+g global • ctrl+r repo • ctrl+d clear session | Reset: ctrl+u field • ctrl+o layer"
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

func (e *configEditor) ResetLayer() {
	e.clearLayerOverrides(e.layer)
}

func (e *configEditor) ResetField() {
	if e.form == nil {
		return
	}
	focused := e.form.GetFocusedField()
	if focused == nil {
		return
	}
	key := focused.GetKey()
	if key == "" || key == fieldLayer {
		return
	}
	base, err := e.baseConfigForLayer(e.layer)
	if err != nil {
		e.saveError = err.Error()
		e.saveNote = ""
		return
	}
	if !e.applyFieldValueFromConfig(key, base) {
		return
	}
	e.syncFocusedFieldValue(focused, key)
	e.saveError = ""
	e.saveNote = fmt.Sprintf("Reset %s", key)
	e.refreshFieldSources()
}

func (e *configEditor) SessionOverrides() config.PartialConfig {
	return e.drafts[layerSession]
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
		if path == "" {
			e.saveError = "repo config path unavailable"
			e.saveNote = ""
			return
		}
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
	e.clearLayerOverrides(layerSession)
}

func (e *configEditor) clearLayerOverrides(layer string) {
	e.drafts[layer] = config.PartialConfig{}
	e.saveError = ""
	switch layer {
	case layerGlobal:
		e.saveNote = "Cleared global overrides"
	case layerRepo:
		e.saveNote = "Cleared repo overrides"
	case layerSession:
		e.saveNote = "Cleared session overrides"
	default:
		e.saveNote = "Cleared overrides"
	}
	if e.layer == layer {
		_ = e.resetLayer(layer)
	}
}

func (e *configEditor) resetLayer(layer string) error {
	cfg, err := e.effectiveConfig(layer)
	if err != nil {
		return err
	}
	e.layer = layer
	e.data = formDataFromConfig(cfg)
	e.form = e.buildForm()
	e.Resize(e.width, e.height)
	e.refreshFieldSources()
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

func (e *configEditor) registerFieldDesc(key string, setter func(string)) {
	if key == "" || setter == nil {
		return
	}
	if e.fieldDescs == nil {
		e.fieldDescs = map[string]func(string){}
	}
	e.fieldDescs[key] = setter
}

func (e *configEditor) refreshFieldSources() {
	if e.form == nil {
		return
	}
	sources, err := e.computeFieldSources()
	if err != nil {
		return
	}
	e.sources = sources
	for key, setter := range e.fieldDescs {
		setter(fmt.Sprintf("Source: %s", e.sourceForKey(key)))
	}
}

func (e *configEditor) computeFieldSources() (config.FieldSources, error) {
	defaults, globalCfg, repoCfg, cliCfg, sessionCfg, err := e.layerConfigsForSources()
	if err != nil {
		return config.FieldSources{}, err
	}
	return config.FieldSourcesForConfigs(defaults, globalCfg, repoCfg, cliCfg, sessionCfg), nil
}

func (e *configEditor) layerConfigs() (config.Config, config.Config, config.Config, config.Config, config.Config, error) {
	defaults, err := config.DefaultConfig()
	if err != nil {
		return config.Config{}, config.Config{}, config.Config{}, config.Config{}, config.Config{}, err
	}
	repoRoot := e.locations.RepoRoot
	if repoRoot == "" {
		repoRoot = e.locations.CWD
	}
	defaults = config.ResolvePaths(defaults, repoRoot, repoRoot)

	globalCfg, err := config.ApplyPartial(defaults, e.drafts[layerGlobal], e.locations.HomeDir, repoRoot)
	if err != nil {
		return config.Config{}, config.Config{}, config.Config{}, config.Config{}, config.Config{}, err
	}
	repoCfg, err := config.ApplyPartial(globalCfg, e.drafts[layerRepo], repoRoot, repoRoot)
	if err != nil {
		return config.Config{}, config.Config{}, config.Config{}, config.Config{}, config.Config{}, err
	}
	cliCfg, err := config.ApplyPartial(repoCfg, e.cliOverrides, e.locations.CWD, repoRoot)
	if err != nil {
		return config.Config{}, config.Config{}, config.Config{}, config.Config{}, config.Config{}, err
	}
	sessionCfg, err := config.ApplyPartial(cliCfg, e.drafts[layerSession], e.locations.CWD, repoRoot)
	if err != nil {
		return config.Config{}, config.Config{}, config.Config{}, config.Config{}, config.Config{}, err
	}
	return defaults, globalCfg, repoCfg, cliCfg, sessionCfg, nil
}

func (e *configEditor) layerConfigsForSources() (config.Config, config.Config, config.Config, config.Config, config.Config, error) {
	defaults, globalCfg, repoCfg, cliCfg, sessionCfg, err := e.layerConfigs()
	if err != nil {
		return config.Config{}, config.Config{}, config.Config{}, config.Config{}, config.Config{}, err
	}

	repoRoot := e.locations.RepoRoot
	if repoRoot == "" {
		repoRoot = e.locations.CWD
	}

	baseCfg := defaults
	basePath := e.locations.HomeDir
	switch e.layer {
	case layerRepo:
		baseCfg = globalCfg
		basePath = repoRoot
	case layerSession:
		baseCfg = cliCfg
		basePath = e.locations.CWD
	default:
		baseCfg = defaults
		basePath = e.locations.HomeDir
	}

	if partial, err := partialFromForm(e.data); err == nil {
		if currentCfg, err := config.ApplyPartial(baseCfg, partial, basePath, repoRoot); err == nil {
			switch e.layer {
			case layerGlobal:
				globalCfg = currentCfg
			case layerRepo:
				repoCfg = currentCfg
			case layerSession:
				sessionCfg = currentCfg
			}
		}
	}

	switch e.layer {
	case layerGlobal:
		repoCfg = globalCfg
		cliCfg = globalCfg
		sessionCfg = globalCfg
	case layerRepo:
		cliCfg = repoCfg
		sessionCfg = repoCfg
	}

	return defaults, globalCfg, repoCfg, cliCfg, sessionCfg, nil
}

func (e *configEditor) baseConfigForLayer(layer string) (config.Config, error) {
	defaults, globalCfg, repoCfg, cliCfg, _, err := e.layerConfigs()
	if err != nil {
		return config.Config{}, err
	}
	switch layer {
	case layerGlobal:
		return defaults, nil
	case layerRepo:
		return globalCfg, nil
	case layerSession:
		return cliCfg, nil
	default:
		return repoCfg, nil
	}
}

func (e *configEditor) sourceForKey(key string) config.SourceLayer {
	switch key {
	case fieldUITheme:
		return e.sources.UITheme
	case fieldUIRefreshSeconds:
		return e.sources.UIRefreshSeconds
	case fieldLogLevel:
		return e.sources.LoggingLevel
	case fieldLogFile:
		return e.sources.LoggingFile
	case fieldLogRedactionMode:
		return e.sources.LoggingRedaction
	case fieldDataDir:
		return e.sources.PathsDataDir
	case fieldCacheDir:
		return e.sources.PathsCacheDir
	case fieldPinDir:
		return e.sources.PathsPinDir
	case fieldSpecsAutofillScout:
		return e.sources.SpecsAutofill
	case fieldSpecsScoutWorkflow:
		return e.sources.SpecsScout
	case fieldSpecsUserFocus:
		return e.sources.SpecsUserFocus
	case fieldSpecsRunner:
		return e.sources.SpecsRunner
	case fieldSpecsRunnerArgs:
		return e.sources.SpecsRunnerArgs
	case fieldSpecsEffort:
		return e.sources.SpecsEffort
	case fieldLoopSleepSeconds:
		return e.sources.LoopSleepSeconds
	case fieldLoopMaxIterations:
		return e.sources.LoopMaxIterations
	case fieldLoopMaxStalled:
		return e.sources.LoopMaxStalled
	case fieldLoopMaxRepair:
		return e.sources.LoopMaxRepair
	case fieldLoopOnlyTags:
		return e.sources.LoopOnlyTags
	case fieldLoopRequireMain:
		return e.sources.LoopRequireMain
	case fieldLoopRunner:
		return e.sources.LoopRunner
	case fieldLoopRunnerArgs:
		return e.sources.LoopRunnerArgs
	case fieldLoopEffort:
		return e.sources.LoopEffort
	case fieldGitAutoCommit:
		return e.sources.GitAutoCommit
	case fieldGitAutoPush:
		return e.sources.GitAutoPush
	default:
		return config.SourceDefault
	}
}

func (e *configEditor) applyFieldValueFromConfig(key string, cfg config.Config) bool {
	switch key {
	case fieldUITheme:
		e.data.UITheme = cfg.UI.Theme
	case fieldUIRefreshSeconds:
		e.data.RefreshSeconds = strconv.Itoa(cfg.UI.RefreshSeconds)
	case fieldLogLevel:
		e.data.LogLevel = cfg.Logging.Level
	case fieldLogRedactionMode:
		e.data.LogRedactionMode = string(cfg.Logging.RedactionMode)
	case fieldLogFile:
		e.data.LogFile = cfg.Logging.File
	case fieldDataDir:
		e.data.DataDir = cfg.Paths.DataDir
	case fieldCacheDir:
		e.data.CacheDir = cfg.Paths.CacheDir
	case fieldPinDir:
		e.data.PinDir = cfg.Paths.PinDir
	case fieldSpecsAutofillScout:
		e.data.SpecsAutofill = cfg.Specs.AutofillScout
	case fieldSpecsScoutWorkflow:
		e.data.SpecsScout = cfg.Specs.ScoutWorkflow
	case fieldSpecsUserFocus:
		e.data.SpecsUserFocus = cfg.Specs.UserFocus
	case fieldSpecsRunner:
		e.data.SpecsRunner = cfg.Specs.Runner
	case fieldSpecsRunnerArgs:
		e.data.SpecsRunnerArgs = formatArgsLines(cfg.Specs.RunnerArgs)
	case fieldSpecsEffort:
		e.data.SpecsEffort = runnerargs.DisplayEffort(cfg.Specs.ReasoningEffort)
	case fieldLoopSleepSeconds:
		e.data.LoopSleepSeconds = strconv.Itoa(cfg.Loop.SleepSeconds)
	case fieldLoopMaxIterations:
		e.data.LoopMaxIterations = strconv.Itoa(cfg.Loop.MaxIterations)
	case fieldLoopMaxStalled:
		e.data.LoopMaxStalled = strconv.Itoa(cfg.Loop.MaxStalled)
	case fieldLoopMaxRepair:
		e.data.LoopMaxRepair = strconv.Itoa(cfg.Loop.MaxRepairAttempts)
	case fieldLoopOnlyTags:
		e.data.LoopOnlyTags = cfg.Loop.OnlyTags
	case fieldLoopRequireMain:
		e.data.LoopRequireMain = cfg.Loop.RequireMain
	case fieldLoopRunner:
		e.data.LoopRunner = cfg.Loop.Runner
	case fieldLoopRunnerArgs:
		e.data.LoopRunnerArgs = formatArgsLines(cfg.Loop.RunnerArgs)
	case fieldLoopEffort:
		e.data.LoopEffort = runnerargs.DisplayEffort(cfg.Loop.ReasoningEffort)
	case fieldGitAutoCommit:
		e.data.GitAutoCommit = cfg.Git.AutoCommit
	case fieldGitAutoPush:
		e.data.GitAutoPush = cfg.Git.AutoPush
	default:
		return false
	}
	return true
}

func (e *configEditor) syncFocusedFieldValue(field huh.Field, key string) {
	switch key {
	case fieldSpecsAutofillScout:
		if confirm, ok := field.(*huh.Confirm); ok {
			confirm.Value(&e.data.SpecsAutofill)
		}
	case fieldSpecsScoutWorkflow:
		if confirm, ok := field.(*huh.Confirm); ok {
			confirm.Value(&e.data.SpecsScout)
		}
	case fieldLoopRequireMain:
		if confirm, ok := field.(*huh.Confirm); ok {
			confirm.Value(&e.data.LoopRequireMain)
		}
	case fieldGitAutoCommit:
		if confirm, ok := field.(*huh.Confirm); ok {
			confirm.Value(&e.data.GitAutoCommit)
		}
	case fieldGitAutoPush:
		if confirm, ok := field.(*huh.Confirm); ok {
			confirm.Value(&e.data.GitAutoPush)
		}
	case fieldLogRedactionMode, fieldSpecsRunner, fieldSpecsEffort, fieldLoopRunner, fieldLoopEffort:
		if selectField, ok := field.(*huh.Select[string]); ok {
			switch key {
			case fieldLogRedactionMode:
				selectField.Value(&e.data.LogRedactionMode)
			case fieldSpecsRunner:
				selectField.Value(&e.data.SpecsRunner)
			case fieldSpecsEffort:
				selectField.Value(&e.data.SpecsEffort)
			case fieldLoopRunner:
				selectField.Value(&e.data.LoopRunner)
			case fieldLoopEffort:
				selectField.Value(&e.data.LoopEffort)
			}
		}
	case fieldSpecsRunnerArgs, fieldLoopRunnerArgs:
		if textField, ok := field.(*huh.Text); ok {
			if key == fieldSpecsRunnerArgs {
				textField.Value(&e.data.SpecsRunnerArgs)
			} else {
				textField.Value(&e.data.LoopRunnerArgs)
			}
		}
	default:
		if input, ok := field.(*huh.Input); ok {
			switch key {
			case fieldUITheme:
				input.Value(&e.data.UITheme)
			case fieldUIRefreshSeconds:
				input.Value(&e.data.RefreshSeconds)
			case fieldLogLevel:
				input.Value(&e.data.LogLevel)
			case fieldLogFile:
				input.Value(&e.data.LogFile)
			case fieldDataDir:
				input.Value(&e.data.DataDir)
			case fieldCacheDir:
				input.Value(&e.data.CacheDir)
			case fieldPinDir:
				input.Value(&e.data.PinDir)
			case fieldSpecsUserFocus:
				input.Value(&e.data.SpecsUserFocus)
			case fieldLoopSleepSeconds:
				input.Value(&e.data.LoopSleepSeconds)
			case fieldLoopMaxIterations:
				input.Value(&e.data.LoopMaxIterations)
			case fieldLoopMaxStalled:
				input.Value(&e.data.LoopMaxStalled)
			case fieldLoopMaxRepair:
				input.Value(&e.data.LoopMaxRepair)
			case fieldLoopOnlyTags:
				input.Value(&e.data.LoopOnlyTags)
			}
		}
	}
}

func (e *configEditor) effectiveConfig(layer string) (config.Config, error) {
	_, globalCfg, repoCfg, _, sessionCfg, err := e.layerConfigs()
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
	base, err := e.baseConfigForLayer(layer)
	if err != nil {
		return err
	}
	repoRoot := e.locations.RepoRoot
	if repoRoot == "" {
		repoRoot = e.locations.CWD
	}

	basePath := e.locations.CWD
	switch layer {
	case layerGlobal:
		basePath = e.locations.HomeDir
	case layerRepo:
		basePath = repoRoot
	}

	cfg, err := config.ApplyPartial(base, partial, basePath, repoRoot)
	if err != nil {
		return err
	}
	return cfg.Validate()
}

func (e *configEditor) buildForm() *huh.Form {
	e.fieldDescs = map[string]func(string){}

	layerField := huh.NewSelect[string]().
		Title("Layer").
		Options(
			huh.NewOption("Global", layerGlobal),
			huh.NewOption("Repo", layerRepo),
			huh.NewOption("Session", layerSession),
		).
		Value(&e.layer).
		Key(fieldLayer)

	uiTheme := huh.NewInput().Title("UI Theme").Value(&e.data.UITheme).Validate(nonEmptyString("ui.theme")).Key(fieldUITheme)
	e.registerFieldDesc(fieldUITheme, func(desc string) { uiTheme.Description(desc) })
	refreshSeconds := huh.NewInput().Title("Refresh Seconds").Value(&e.data.RefreshSeconds).Validate(positiveInt("ui.refresh_seconds")).Key(fieldUIRefreshSeconds)
	e.registerFieldDesc(fieldUIRefreshSeconds, func(desc string) { refreshSeconds.Description(desc) })
	logLevel := huh.NewInput().Title("Log Level").Value(&e.data.LogLevel).Validate(logLevelValidator()).Key(fieldLogLevel)
	e.registerFieldDesc(fieldLogLevel, func(desc string) { logLevel.Description(desc) })
	logRedaction := huh.NewSelect[string]().
		Title("Log Redaction Mode").
		Options(
			huh.NewOption("secrets_only (default)", string(redaction.ModeSecretsOnly)),
			huh.NewOption("all_env", string(redaction.ModeAllEnv)),
			huh.NewOption("off", string(redaction.ModeOff)),
		).
		Value(&e.data.LogRedactionMode).
		Key(fieldLogRedactionMode)
	e.registerFieldDesc(fieldLogRedactionMode, func(desc string) { logRedaction.Description(desc) })
	logFile := huh.NewInput().Title("Log File").Value(&e.data.LogFile).Key(fieldLogFile)
	e.registerFieldDesc(fieldLogFile, func(desc string) { logFile.Description(desc) })

	dataDir := huh.NewInput().Title("Data Dir").Value(&e.data.DataDir).Validate(nonEmptyString("paths.data_dir")).Key(fieldDataDir)
	e.registerFieldDesc(fieldDataDir, func(desc string) { dataDir.Description(desc) })
	cacheDir := huh.NewInput().Title("Cache Dir").Value(&e.data.CacheDir).Validate(nonEmptyString("paths.cache_dir")).Key(fieldCacheDir)
	e.registerFieldDesc(fieldCacheDir, func(desc string) { cacheDir.Description(desc) })
	pinDir := huh.NewInput().Title("Pin Dir").Value(&e.data.PinDir).Validate(nonEmptyString("paths.pin_dir")).Key(fieldPinDir)
	e.registerFieldDesc(fieldPinDir, func(desc string) { pinDir.Description(desc) })

	specsAutofill := huh.NewConfirm().Title("Specs Autofill Scout").Value(&e.data.SpecsAutofill).Key(fieldSpecsAutofillScout)
	e.registerFieldDesc(fieldSpecsAutofillScout, func(desc string) { specsAutofill.Description(desc) })
	specsScout := huh.NewConfirm().Title("Specs Scout Workflow").Value(&e.data.SpecsScout).Key(fieldSpecsScoutWorkflow)
	e.registerFieldDesc(fieldSpecsScoutWorkflow, func(desc string) { specsScout.Description(desc) })
	specsUserFocus := huh.NewInput().Title("Specs User Focus").Value(&e.data.SpecsUserFocus).Key(fieldSpecsUserFocus)
	e.registerFieldDesc(fieldSpecsUserFocus, func(desc string) { specsUserFocus.Description(desc) })
	specsRunner := huh.NewSelect[string]().
		Title("Specs Runner").
		Options(
			huh.NewOption("codex", "codex"),
			huh.NewOption("opencode", "opencode"),
		).
		Value(&e.data.SpecsRunner).
		Key(fieldSpecsRunner)
	e.registerFieldDesc(fieldSpecsRunner, func(desc string) { specsRunner.Description(desc) })
	specsRunnerArgs := huh.NewText().Title("Specs Runner Args (one per line)").Value(&e.data.SpecsRunnerArgs).Lines(3).Key(fieldSpecsRunnerArgs)
	e.registerFieldDesc(fieldSpecsRunnerArgs, func(desc string) { specsRunnerArgs.Description(desc) })
	specsEffort := huh.NewSelect[string]().
		Title("Specs Reasoning Effort").
		Options(
			huh.NewOption("auto", "auto"),
			huh.NewOption("low", "low"),
			huh.NewOption("medium", "medium"),
			huh.NewOption("high", "high"),
			huh.NewOption("off", "off"),
		).
		Value(&e.data.SpecsEffort).
		Key(fieldSpecsEffort)
	e.registerFieldDesc(fieldSpecsEffort, func(desc string) { specsEffort.Description(desc) })

	loopSleepSeconds := huh.NewInput().Title("Sleep Seconds").Value(&e.data.LoopSleepSeconds).Validate(nonNegativeInt("loop.sleep_seconds")).Key(fieldLoopSleepSeconds)
	e.registerFieldDesc(fieldLoopSleepSeconds, func(desc string) { loopSleepSeconds.Description(desc) })
	loopMaxIterations := huh.NewInput().Title("Max Iterations").Value(&e.data.LoopMaxIterations).Validate(nonNegativeInt("loop.max_iterations")).Key(fieldLoopMaxIterations)
	e.registerFieldDesc(fieldLoopMaxIterations, func(desc string) { loopMaxIterations.Description(desc) })
	loopMaxStalled := huh.NewInput().Title("Max Stalled").Value(&e.data.LoopMaxStalled).Validate(nonNegativeInt("loop.max_stalled")).Key(fieldLoopMaxStalled)
	e.registerFieldDesc(fieldLoopMaxStalled, func(desc string) { loopMaxStalled.Description(desc) })
	loopMaxRepair := huh.NewInput().Title("Max Repair Attempts").Value(&e.data.LoopMaxRepair).Validate(nonNegativeInt("loop.max_repair_attempts")).Key(fieldLoopMaxRepair)
	e.registerFieldDesc(fieldLoopMaxRepair, func(desc string) { loopMaxRepair.Description(desc) })
	loopOnlyTags := huh.NewInput().Title("Only Tags").Value(&e.data.LoopOnlyTags).Key(fieldLoopOnlyTags)
	e.registerFieldDesc(fieldLoopOnlyTags, func(desc string) { loopOnlyTags.Description(desc) })
	loopRequireMain := huh.NewConfirm().Title("Require Main Branch").Value(&e.data.LoopRequireMain).Key(fieldLoopRequireMain)
	e.registerFieldDesc(fieldLoopRequireMain, func(desc string) { loopRequireMain.Description(desc) })
	loopRunner := huh.NewSelect[string]().
		Title("Loop Runner").
		Options(
			huh.NewOption("codex", "codex"),
			huh.NewOption("opencode", "opencode"),
		).
		Value(&e.data.LoopRunner).
		Key(fieldLoopRunner)
	e.registerFieldDesc(fieldLoopRunner, func(desc string) { loopRunner.Description(desc) })
	loopRunnerArgs := huh.NewText().Title("Loop Runner Args (one per line)").Value(&e.data.LoopRunnerArgs).Lines(3).Key(fieldLoopRunnerArgs)
	e.registerFieldDesc(fieldLoopRunnerArgs, func(desc string) { loopRunnerArgs.Description(desc) })
	loopEffort := huh.NewSelect[string]().
		Title("Loop Reasoning Effort").
		Options(
			huh.NewOption("auto", "auto"),
			huh.NewOption("low", "low"),
			huh.NewOption("medium", "medium"),
			huh.NewOption("high", "high"),
			huh.NewOption("off", "off"),
		).
		Value(&e.data.LoopEffort).
		Key(fieldLoopEffort)
	e.registerFieldDesc(fieldLoopEffort, func(desc string) { loopEffort.Description(desc) })

	gitAutoCommit := huh.NewConfirm().Title("Git Auto Commit").Value(&e.data.GitAutoCommit).Key(fieldGitAutoCommit)
	e.registerFieldDesc(fieldGitAutoCommit, func(desc string) { gitAutoCommit.Description(desc) })
	gitAutoPush := huh.NewConfirm().Title("Git Auto Push").Value(&e.data.GitAutoPush).Key(fieldGitAutoPush)
	e.registerFieldDesc(fieldGitAutoPush, func(desc string) { gitAutoPush.Description(desc) })

	return huh.NewForm(
		huh.NewGroup(
			layerField,
			uiTheme,
			refreshSeconds,
			logLevel,
			logRedaction,
			logFile,
			dataDir,
			cacheDir,
			pinDir,
			specsAutofill,
			specsScout,
			specsUserFocus,
			specsRunner,
			specsRunnerArgs,
			specsEffort,
			loopSleepSeconds,
			loopMaxIterations,
			loopMaxStalled,
			loopMaxRepair,
			loopOnlyTags,
			loopRequireMain,
			loopRunner,
			loopRunnerArgs,
			loopEffort,
			gitAutoCommit,
			gitAutoPush,
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
		SpecsAutofill:     cfg.Specs.AutofillScout,
		SpecsScout:        cfg.Specs.ScoutWorkflow,
		SpecsUserFocus:    cfg.Specs.UserFocus,
		SpecsRunner:       cfg.Specs.Runner,
		SpecsRunnerArgs:   formatArgsLines(cfg.Specs.RunnerArgs),
		SpecsEffort:       runnerargs.DisplayEffort(cfg.Specs.ReasoningEffort),
		LoopSleepSeconds:  strconv.Itoa(cfg.Loop.SleepSeconds),
		LoopMaxIterations: strconv.Itoa(cfg.Loop.MaxIterations),
		LoopMaxStalled:    strconv.Itoa(cfg.Loop.MaxStalled),
		LoopMaxRepair:     strconv.Itoa(cfg.Loop.MaxRepairAttempts),
		LoopOnlyTags:      cfg.Loop.OnlyTags,
		LoopRequireMain:   cfg.Loop.RequireMain,
		LoopRunner:        cfg.Loop.Runner,
		LoopRunnerArgs:    formatArgsLines(cfg.Loop.RunnerArgs),
		LoopEffort:        runnerargs.DisplayEffort(cfg.Loop.ReasoningEffort),
		GitAutoCommit:     cfg.Git.AutoCommit,
		GitAutoPush:       cfg.Git.AutoPush,
	}
}

func partialFromForm(data configFormData) (config.PartialConfig, error) {
	refreshSeconds, err := parsePositiveInt("ui.refresh_seconds", data.RefreshSeconds)
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
		Specs: &config.SpecsPartial{
			AutofillScout:   &data.SpecsAutofill,
			ScoutWorkflow:   &data.SpecsScout,
			UserFocus:       &data.SpecsUserFocus,
			Runner:          &specsRunner,
			RunnerArgs:      parseArgsLines(data.SpecsRunnerArgs),
			ReasoningEffort: &specsEffort,
		},
		Loop: &config.LoopPartial{
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
			AutoCommit: &data.GitAutoCommit,
			AutoPush:   &data.GitAutoPush,
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
