// Command ralph is the entrypoint for the Ralph CLI and TUI.
// Entrypoint: main.
package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/migrate"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
	"github.com/mitchfultz/ralph/ralph_tui/internal/tui"
	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
	"golang.org/x/term"
)

func main() {
	rootCmd := newRootCommand()
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func newRootCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:           "ralph",
		Short:         "Ralph tools and dashboard",
		Long:          "Ralph launches a lightweight TUI dashboard and exposes configuration utilities.",
		Example:       "  ralph\n  ralph config show\n  ralph --ui-theme solar",
		Args:          cobra.NoArgs,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			locs, err := paths.Resolve("")
			if err != nil {
				return err
			}
			cliOverrides, err := buildCLIOverrides(cmd)
			if err != nil {
				return err
			}
			cfg, err := config.LoadFromLocations(config.LoadOptions{
				Locations:    locs,
				CLIOverrides: cliOverrides,
			})
			if err != nil {
				return err
			}
			return tui.Start(cfg, locs, tui.StartOptions{
				CLIOverrides: cliOverrides,
			})
		},
	}

	cmd.PersistentFlags().String("ui-theme", "", "UI theme name")
	cmd.PersistentFlags().Int("refresh-seconds", 0, "UI refresh interval in seconds")
	cmd.PersistentFlags().String("log-level", "", "Log level (debug, info, warn, error)")
	cmd.PersistentFlags().String("log-file", "", "Log file path")
	cmd.PersistentFlags().String("redaction-mode", "", "Log redaction mode (off, secrets_only, all_env)")
	cmd.PersistentFlags().Int("log-max-buffered-bytes", 0, "Max bytes buffered per log line before partial flush (0 = disable)")
	cmd.PersistentFlags().String("project-type", "", "Project type (code, docs)")
	cmd.PersistentFlags().String("data-dir", "", "Data directory path")
	cmd.PersistentFlags().String("cache-dir", "", "Cache directory path")
	cmd.PersistentFlags().String("pin-dir", "", "Pin directory path")

	cmd.AddCommand(newConfigCommand())
	cmd.AddCommand(newInitCommand())
	cmd.AddCommand(newMigrateCommand())
	cmd.AddCommand(newPinCommand())
	cmd.AddCommand(newSpecsCommand())
	cmd.AddCommand(newLoopCommand())

	return cmd
}

func newConfigCommand() *cobra.Command {
	configCmd := &cobra.Command{
		Use:     "config",
		Short:   "Configuration helpers",
		Long:    "Inspect or validate the Ralph configuration.",
		Example: "  ralph config show\n  ralph --log-level debug config show",
	}

	configCmd.AddCommand(newConfigShowCommand())

	return configCmd
}

func newMigrateCommand() *cobra.Command {
	return &cobra.Command{
		Use:     "migrate",
		Short:   "Migrate Ralph pin files to the final layout",
		Long:    "Move ralph_legacy/specs pin files into .ralph/pin, update repo config, and validate the pin.",
		Example: "  ralph migrate",
		Args:    cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			locs, err := paths.Resolve("")
			if err != nil {
				return err
			}
			result, err := migrate.Run(locs.RepoRoot, locs.RepoConfigPath)
			if err != nil {
				return err
			}

			out := cmd.OutOrStdout()
			if len(result.Moved) > 0 {
				_, _ = fmt.Fprintf(out, ">> [RALPH] Moved %d pin files to %s.\n", len(result.Moved), result.NewPinDir)
			} else {
				_, _ = fmt.Fprintf(out, ">> [RALPH] Pin already located at %s.\n", result.NewPinDir)
			}
			_, _ = fmt.Fprintf(out, ">> [RALPH] Updated %s (paths.pin_dir=%s).\n", result.ConfigPath, result.ConfigPinDir)
			_, err = fmt.Fprintln(out, ">> [RALPH] Pin validation OK.")
			return err
		},
	}
}

func newInitCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "init",
		Short:   "Initialize Ralph pin and cache directories",
		Long:    "Create the .ralph/pin and .ralph/cache layouts and seed default pin files.",
		Example: "  ralph init\n  ralph init --project-type docs\n  ralph init --force",
		Args:    cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			locs, err := paths.Resolve("")
			if err != nil {
				return err
			}
			cliOverrides, err := buildCLIOverrides(cmd)
			if err != nil {
				return err
			}
			cfg, err := config.LoadFromLocations(config.LoadOptions{
				Locations:    locs,
				CLIOverrides: cliOverrides,
			})
			if err != nil {
				return err
			}
			projectType, err := resolveInitProjectType(cmd, locs, cfg)
			if err != nil {
				return err
			}
			force, err := cmd.Flags().GetBool("force")
			if err != nil {
				return err
			}
			result, err := pin.InitLayout(cfg.Paths.PinDir, cfg.Paths.CacheDir, pin.InitOptions{
				Force:       force,
				ProjectType: projectType,
			})
			if err != nil {
				return err
			}
			files := pin.ResolveFiles(cfg.Paths.PinDir)
			if err := pin.ValidatePin(files, projectType); err != nil {
				return err
			}
			if err := persistProjectTypeConfig(locs.RepoConfigPath, locs.RepoRoot, projectType); err != nil {
				return err
			}

			out := cmd.OutOrStdout()
			if len(result.Created) == 0 && len(result.Overwritten) == 0 {
				_, _ = fmt.Fprintf(out, ">> [RALPH] Pin already initialized at %s.\n", result.PinDir)
			} else {
				_, _ = fmt.Fprintf(out, ">> [RALPH] Pin initialized at %s.\n", result.PinDir)
			}
			if len(result.Created) > 0 {
				_, _ = fmt.Fprintf(out, ">> [RALPH] Created: %s\n", strings.Join(result.Created, ", "))
			}
			if len(result.Overwritten) > 0 {
				_, _ = fmt.Fprintf(out, ">> [RALPH] Overwritten: %s\n", strings.Join(result.Overwritten, ", "))
			}
			_, err = fmt.Fprintf(out, ">> [RALPH] Cache directory: %s\n", result.CacheDir)
			return err
		},
	}
	cmd.Flags().Bool("force", false, "Overwrite existing pin files")
	return cmd
}

func resolveInitProjectType(cmd *cobra.Command, locs paths.Locations, cfg config.Config) (project.Type, error) {
	flags := cmd.Flags()
	if flags.Changed("project-type") {
		value, err := flags.GetString("project-type")
		if err != nil {
			return "", err
		}
		projectType := project.NormalizeType(value)
		if projectType == "" {
			projectType = project.DefaultType()
		}
		if !project.ValidType(projectType) {
			return "", fmt.Errorf("project_type must be code or docs")
		}
		return projectType, nil
	}

	if locs.RepoConfigPath != "" {
		partial, err := config.LoadPartial(locs.RepoConfigPath)
		if err != nil {
			return "", err
		}
		if partial != nil && partial.ProjectType != nil {
			projectType := project.NormalizeType(string(*partial.ProjectType))
			if projectType == "" {
				projectType = project.DefaultType()
			}
			if !project.ValidType(projectType) {
				return "", fmt.Errorf("project_type must be code or docs")
			}
			return projectType, nil
		}
	}

	projectType := project.NormalizeType(string(cfg.ProjectType))
	if projectType == "" {
		projectType = project.DefaultType()
	}
	if !project.ValidType(projectType) {
		return "", fmt.Errorf("project_type must be code or docs")
	}

	if !isTerminal(cmd.InOrStdin()) {
		return projectType, nil
	}

	detected, summary, err := project.DetectType(locs.RepoRoot)
	if err != nil {
		return projectType, nil
	}
	if detected == "" {
		detected = projectType
	}
	return promptProjectType(cmd.InOrStdin(), cmd.OutOrStdout(), detected, summary)
}

func promptProjectType(in io.Reader, out io.Writer, defaultType project.Type, summary project.DetectSummary) (project.Type, error) {
	reader := bufio.NewReader(in)
	if summary.CodeFiles > 0 || summary.DocsFiles > 0 {
		_, _ = fmt.Fprintf(out, "Detected %d code files and %d docs files.\n", summary.CodeFiles, summary.DocsFiles)
	}
	for {
		_, _ = fmt.Fprintf(out, "Select project type [code/docs] (default: %s): ", defaultType)
		line, err := reader.ReadString('\n')
		if err != nil && err != io.EOF {
			return "", err
		}
		choice := project.NormalizeType(line)
		if choice == "" {
			return defaultType, nil
		}
		if project.ValidType(choice) {
			return choice, nil
		}
		_, _ = fmt.Fprintln(out, "Invalid selection. Enter \"code\" or \"docs\".")
		if err == io.EOF {
			return defaultType, nil
		}
	}
}

func persistProjectTypeConfig(repoConfigPath string, repoRoot string, projectType project.Type) error {
	if repoConfigPath == "" {
		return fmt.Errorf("repo config path unavailable")
	}
	normalized := project.NormalizeType(string(projectType))
	if normalized == "" {
		normalized = project.DefaultType()
	}
	if !project.ValidType(normalized) {
		return fmt.Errorf("project_type must be code or docs")
	}
	partial, err := config.LoadPartial(repoConfigPath)
	if err != nil {
		return err
	}
	if partial == nil {
		partial = &config.PartialConfig{}
	}
	if partial.ProjectType != nil {
		existing := project.NormalizeType(string(*partial.ProjectType))
		if existing == normalized && partial.Version != nil {
			return nil
		}
	}
	partial.ProjectType = &normalized
	if partial.Version == nil {
		version := 1
		partial.Version = &version
	}
	return config.SavePartial(repoConfigPath, *partial, config.SaveOptions{RelativeRoot: repoRoot})
}

func isTerminal(reader io.Reader) bool {
	file, ok := reader.(*os.File)
	if !ok {
		return false
	}
	return term.IsTerminal(int(file.Fd()))
}

func newConfigShowCommand() *cobra.Command {
	return &cobra.Command{
		Use:     "show",
		Short:   "Print the effective configuration JSON",
		Long:    "Print the merged, effective Ralph configuration as JSON.",
		Example: "  ralph config show\n  ralph --ui-theme solar config show",
		Args:    cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			payload, err := json.MarshalIndent(cfg, "", "  ")
			if err != nil {
				return err
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), string(payload))
			return err
		},
	}
}

func loadConfig(cmd *cobra.Command) (config.Config, error) {
	cliOverrides, err := buildCLIOverrides(cmd)
	if err != nil {
		return config.Config{}, err
	}

	return config.Load("", config.PartialConfig{}, cliOverrides)
}

func buildCLIOverrides(cmd *cobra.Command) (config.PartialConfig, error) {
	flags := cmd.Flags()
	var overrides config.PartialConfig

	if flags.Changed("ui-theme") {
		value, err := flags.GetString("ui-theme")
		if err != nil {
			return overrides, err
		}
		ui := ensureUIPartial(&overrides)
		ui.Theme = &value
	}
	if flags.Changed("refresh-seconds") {
		value, err := flags.GetInt("refresh-seconds")
		if err != nil {
			return overrides, err
		}
		ui := ensureUIPartial(&overrides)
		ui.RefreshSeconds = &value
	}
	if flags.Changed("log-level") {
		value, err := flags.GetString("log-level")
		if err != nil {
			return overrides, err
		}
		logging := ensureLoggingPartial(&overrides)
		logging.Level = &value
	}
	if flags.Changed("log-file") {
		value, err := flags.GetString("log-file")
		if err != nil {
			return overrides, err
		}
		logging := ensureLoggingPartial(&overrides)
		logging.File = &value
	}
	if flags.Changed("redaction-mode") {
		value, err := flags.GetString("redaction-mode")
		if err != nil {
			return overrides, err
		}
		mode := redaction.Mode(value)
		logging := ensureLoggingPartial(&overrides)
		logging.RedactionMode = &mode
	}
	if flags.Changed("log-max-buffered-bytes") {
		value, err := flags.GetInt("log-max-buffered-bytes")
		if err != nil {
			return overrides, err
		}
		logging := ensureLoggingPartial(&overrides)
		logging.MaxBufferedBytes = &value
	}
	if flags.Changed("project-type") {
		value, err := flags.GetString("project-type")
		if err != nil {
			return overrides, err
		}
		projectType := project.Type(value)
		overrides.ProjectType = &projectType
	}
	if flags.Changed("data-dir") {
		value, err := flags.GetString("data-dir")
		if err != nil {
			return overrides, err
		}
		pathsPartial := ensurePathsPartial(&overrides)
		pathsPartial.DataDir = &value
	}
	if flags.Changed("cache-dir") {
		value, err := flags.GetString("cache-dir")
		if err != nil {
			return overrides, err
		}
		pathsPartial := ensurePathsPartial(&overrides)
		pathsPartial.CacheDir = &value
	}
	if flags.Changed("pin-dir") {
		value, err := flags.GetString("pin-dir")
		if err != nil {
			return overrides, err
		}
		pathsPartial := ensurePathsPartial(&overrides)
		pathsPartial.PinDir = &value
	}

	return overrides, nil
}

func ensureUIPartial(overrides *config.PartialConfig) *config.UIPartial {
	if overrides.UI == nil {
		overrides.UI = &config.UIPartial{}
	}
	return overrides.UI
}

func ensureLoggingPartial(overrides *config.PartialConfig) *config.LoggingPartial {
	if overrides.Logging == nil {
		overrides.Logging = &config.LoggingPartial{}
	}
	return overrides.Logging
}

func ensurePathsPartial(overrides *config.PartialConfig) *config.PathsPartial {
	if overrides.Paths == nil {
		overrides.Paths = &config.PathsPartial{}
	}
	return overrides.Paths
}

func newSpecsCommand() *cobra.Command {
	specsCmd := &cobra.Command{
		Use:     "specs",
		Short:   "Specs builder tools",
		Long:    "Build or preview Ralph specs via the prompt template and runner.",
		Example: "  ralph specs build\n  ralph specs build --interactive\n  ralph specs build --runner opencode -- --agent default",
	}

	specsCmd.AddCommand(newSpecsBuildCommand())

	return specsCmd
}

func newSpecsBuildCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "build",
		Short:   "Build or refresh specs",
		Long:    "Build Ralph specs using the prompt template and the selected runner.",
		Example: "  ralph specs build\n  ralph specs build --print-prompt\n  ralph specs build --runner opencode -- --agent default",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			locs, err := paths.Resolve("")
			if err != nil {
				return err
			}

			flags := cmd.Flags()
			interactive, err := flags.GetBool("interactive")
			if err != nil {
				return err
			}
			innovate, err := flags.GetBool("innovate")
			if err != nil {
				return err
			}
			printPrompt, err := flags.GetBool("print-prompt")
			if err != nil {
				return err
			}
			autofillScoutFlag, err := flags.GetBool("autofill-scout")
			if err != nil {
				return err
			}
			noAutofillScoutFlag, err := flags.GetBool("no-autofill-scout")
			if err != nil {
				return err
			}
			if autofillScoutFlag && noAutofillScoutFlag {
				return fmt.Errorf("cannot set both --autofill-scout and --no-autofill-scout")
			}
			var overrides specs.BuildOptionOverrides
			if flags.Changed("innovate") {
				overrides.Innovate = &innovate
			}
			if autofillScoutFlag {
				value := true
				overrides.AutofillScout = &value
			} else if noAutofillScoutFlag {
				value := false
				overrides.AutofillScout = &value
			}
			if flags.Changed("scout-workflow") {
				value, err := flags.GetBool("scout-workflow")
				if err != nil {
					return err
				}
				overrides.ScoutWorkflow = &value
			}
			if flags.Changed("user-focus") {
				value, err := flags.GetString("user-focus")
				if err != nil {
					return err
				}
				overrides.UserFocus = &value
			}
			if flags.Changed("runner") {
				value, err := flags.GetString("runner")
				if err != nil {
					return err
				}
				runner := specs.Runner(runnerargs.NormalizeRunner(value))
				overrides.Runner = &runner
			}

			promptPath, err := flags.GetString("prompt")
			if err != nil {
				return err
			}

			resolved := specs.ResolveBuildOptions(
				specs.BuildOptionDefaults{
					Innovate:        false,
					AutofillScout:   cfg.Specs.AutofillScout,
					ScoutWorkflow:   cfg.Specs.ScoutWorkflow,
					UserFocus:       cfg.Specs.UserFocus,
					Runner:          specs.Runner(cfg.Specs.Runner),
					RunnerArgs:      cfg.Specs.RunnerArgs,
					ReasoningEffort: cfg.Specs.ReasoningEffort,
				},
				overrides,
				flags.Args(),
			)

			result, err := specs.Build(context.Background(), specs.BuildOptions{
				RepoRoot:         locs.RepoRoot,
				PinDir:           cfg.Paths.PinDir,
				PromptTemplate:   promptPath,
				ProjectType:      cfg.ProjectType,
				Runner:           resolved.Runner,
				RunnerArgs:       resolved.RunnerArgs,
				Interactive:      interactive,
				Innovate:         resolved.Innovate,
				InnovateExplicit: resolved.InnovateExplicit,
				AutofillScout:    resolved.AutofillScout,
				ScoutWorkflow:    resolved.ScoutWorkflow,
				UserFocus:        resolved.UserFocus,
				PrintPrompt:      printPrompt,
			})
			if err != nil {
				return err
			}
			if printPrompt {
				_, err = fmt.Fprintln(cmd.OutOrStdout(), result.Prompt)
				return err
			}

			files := pin.ResolveFiles(cfg.Paths.PinDir)
			if err := pin.ValidatePin(files, cfg.ProjectType); err != nil {
				return err
			}
			if _, err := fmt.Fprintln(cmd.OutOrStdout(), ">> [RALPH] Pin validation OK."); err != nil {
				return err
			}

			diffStat, err := specs.GitDiffStat(locs.RepoRoot)
			if err == nil && diffStat != "" {
				_, _ = fmt.Fprintln(cmd.OutOrStdout(), diffStat)
			}
			return nil
		},
	}

	cmd.Flags().String("runner", string(specs.RunnerCodex), "Runner to use: codex or opencode")
	cmd.Flags().Bool("interactive", false, "Prompt for approval before adding new queue items")
	cmd.Flags().Bool("innovate", false, "Allow the specs builder to add new queue items directly to Queue")
	cmd.Flags().Bool("autofill-scout", false, "Enable auto-innovate when Queue is empty")
	cmd.Flags().Bool("no-autofill-scout", false, "Disable auto-innovate when Queue is empty")
	cmd.Flags().Bool("scout-workflow", false, "Include scout workflow instructions in the prompt")
	cmd.Flags().String("user-focus", "", "Optional focus prompt to guide the scout workflow")
	cmd.Flags().Bool("print-prompt", false, "Print the filled prompt and exit")
	cmd.Flags().String("prompt", "", "Prompt template path (default: .ralph/pin/specs_builder.md or specs_builder_docs.md by project type)")

	return cmd
}

func newLoopCommand() *cobra.Command {
	loopCmd := &cobra.Command{
		Use:     "loop",
		Short:   "Run the Ralph supervised loop",
		Long:    "Run the Ralph loop for Codex or opencode with quarantine and supervisor support.",
		Example: "  ralph loop run --once\n  ralph loop run --max-iterations 10 --sleep 2",
	}

	loopCmd.AddCommand(newLoopRunCommand())
	loopCmd.AddCommand(newLoopFixupCommand())

	return loopCmd
}

func newLoopRunCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "run",
		Short:   "Run the supervised loop",
		Long:    "Run the Ralph worker loop, enforcing pin invariants and quarantine rules.",
		Example: "  ralph loop run --once\n  ralph loop run --only-tag db,ui\n  ralph loop run --force-context-builder --reasoning-effort medium",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			locs, err := paths.Resolve("")
			if err != nil {
				return err
			}

			flags := cmd.Flags()

			runnerName, err := resolveRunnerFlag(flags, "runner", cfg.Loop.Runner)
			if err != nil {
				return err
			}

			promptPath := ""
			if flags.Changed("prompt") {
				value, err := flags.GetString("prompt")
				if err != nil {
					return err
				}
				promptPath = value
			}

			supervisorPrompt := ""
			if flags.Changed("supervisor-prompt") {
				value, err := flags.GetString("supervisor-prompt")
				if err != nil {
					return err
				}
				supervisorPrompt = value
			}

			sleepSeconds := cfg.Loop.SleepSeconds
			if flags.Changed("sleep") {
				value, err := flags.GetInt("sleep")
				if err != nil {
					return err
				}
				sleepSeconds = value
			}

			maxIterations := cfg.Loop.MaxIterations
			if flags.Changed("max-iterations") {
				value, err := flags.GetInt("max-iterations")
				if err != nil {
					return err
				}
				maxIterations = value
			}

			maxStalled := cfg.Loop.MaxStalled
			if flags.Changed("max-stalled") {
				value, err := flags.GetInt("max-stalled")
				if err != nil {
					return err
				}
				maxStalled = value
			}

			maxRepair := cfg.Loop.MaxRepairAttempts
			if flags.Changed("max-repair-attempts") {
				value, err := flags.GetInt("max-repair-attempts")
				if err != nil {
					return err
				}
				maxRepair = value
			}
			runnerInactivity := cfg.Loop.RunnerInactivitySeconds
			if flags.Changed("runner-inactivity-seconds") {
				value, err := flags.GetInt("runner-inactivity-seconds")
				if err != nil {
					return err
				}
				runnerInactivity = value
			}

			onlyTags := cfg.Loop.OnlyTags
			if flags.Changed("only-tag") {
				value, err := flags.GetString("only-tag")
				if err != nil {
					return err
				}
				onlyTags = value
			}

			startPolicy := cfg.Loop.DirtyRepo.StartPolicy
			if flags.Changed("dirty-start-policy") {
				value, err := flags.GetString("dirty-start-policy")
				if err != nil {
					return err
				}
				startPolicy = value
			}
			duringPolicy := cfg.Loop.DirtyRepo.DuringPolicy
			if flags.Changed("dirty-during-policy") {
				value, err := flags.GetString("dirty-during-policy")
				if err != nil {
					return err
				}
				duringPolicy = value
			}
			dirtyStartPolicy, err := loop.ParseDirtyRepoPolicy(startPolicy)
			if err != nil {
				return err
			}
			dirtyDuringPolicy, err := loop.ParseDirtyRepoPolicy(duringPolicy)
			if err != nil {
				return err
			}

			allowUntracked := cfg.Loop.DirtyRepo.AllowUntracked
			allowUntrackedFlag, err := flags.GetBool("allow-untracked")
			if err != nil {
				return err
			}
			noAllowUntrackedFlag, err := flags.GetBool("no-allow-untracked")
			if err != nil {
				return err
			}
			if allowUntrackedFlag && noAllowUntrackedFlag {
				return fmt.Errorf("cannot set both --allow-untracked and --no-allow-untracked")
			}
			if allowUntrackedFlag {
				allowUntracked = true
			} else if noAllowUntrackedFlag {
				allowUntracked = false
			}

			quarantineClean := cfg.Loop.DirtyRepo.QuarantineCleanUntracked
			quarantineCleanFlag, err := flags.GetBool("quarantine-clean-untracked")
			if err != nil {
				return err
			}
			noQuarantineCleanFlag, err := flags.GetBool("no-quarantine-clean-untracked")
			if err != nil {
				return err
			}
			if quarantineCleanFlag && noQuarantineCleanFlag {
				return fmt.Errorf("cannot set both --quarantine-clean-untracked and --no-quarantine-clean-untracked")
			}
			if quarantineCleanFlag {
				quarantineClean = true
			} else if noQuarantineCleanFlag {
				quarantineClean = false
			}

			runOnce, _ := flags.GetBool("once")
			forceContextBuilder, _ := flags.GetBool("force-context-builder")

			if sleepSeconds < 0 || maxIterations < 0 || maxStalled < 0 || maxRepair < 0 || runnerInactivity < 0 {
				return fmt.Errorf("loop numeric values must be non-negative")
			}

			logger := loop.StdLogger{Writer: cmd.OutOrStdout()}
			effort, err := resolveFlagString(flags, "reasoning-effort", cfg.Loop.ReasoningEffort)
			if err != nil {
				return err
			}
			runnerArgs := mergeRunnerArgsWithEffort(
				runnerName,
				cfg.Loop.RunnerArgs,
				cmd.Flags().Args(),
				effort,
			)
			onlyTagsParsed, err := parseOnlyTagsCLI(onlyTags)
			if err != nil {
				return err
			}
			runner, err := loop.NewRunner(loop.Options{
				RepoRoot:                locs.RepoRoot,
				PinDir:                  cfg.Paths.PinDir,
				PromptPath:              promptPath,
				SupervisorPrompt:        supervisorPrompt,
				ProjectType:             cfg.ProjectType,
				Runner:                  runnerName,
				RunnerArgs:              runnerArgs,
				ReasoningEffort:         effort,
				ForceContextBuilder:     forceContextBuilder,
				SleepSeconds:            sleepSeconds,
				MaxIterations:           maxIterations,
				MaxStalled:              maxStalled,
				MaxRepairAttempts:       maxRepair,
				RunnerInactivitySeconds: runnerInactivity,
				OnlyTags:                onlyTagsParsed,
				Once:                    runOnce,
				RequireMain:             cfg.Loop.RequireMain,
				AutoCommit:              cfg.Git.AutoCommit,
				AutoPush:                cfg.Git.AutoPush,
				DirtyRepoStart:          dirtyStartPolicy,
				DirtyRepoDuring:         dirtyDuringPolicy,
				AllowUntracked:          allowUntracked,
				QuarantineClean:         quarantineClean,
				RedactionMode:           cfg.Logging.RedactionMode,
				LogMaxBufferedBytes:     cfg.Logging.MaxBufferedBytes,
				Logger:                  logger,
			})
			if err != nil {
				return err
			}
			return runner.Run(context.Background())
		},
	}

	cmd.Flags().String("runner", "codex", "Runner to use: codex or opencode")
	cmd.Flags().String("prompt", "", "Path to worker prompt file")
	cmd.Flags().String("supervisor-prompt", "", "Path to supervisor prompt file")
	cmd.Flags().Int("sleep", 5, "Sleep between iterations in seconds")
	cmd.Flags().Int("max-iterations", 0, "Stop after N iterations (0 = infinite)")
	cmd.Flags().Int("max-stalled", 3, "Auto-block after N stalled iterations")
	cmd.Flags().Int("max-repair-attempts", 2, "Supervisor repair attempts before auto-block")
	cmd.Flags().Int("runner-inactivity-seconds", 0, "Cancel and reset when the runner is inactive for N seconds (0 = disabled)")
	cmd.Flags().String("only-tag", "", "Only execute queue items tagged with [tag] (comma/space-separated)")
	cmd.Flags().Bool("once", false, "Run exactly one iteration and exit")
	cmd.Flags().String("reasoning-effort", "", "Codex reasoning effort override (auto/low/medium/high/off)")
	cmd.Flags().Bool("force-context-builder", false, "Force context_builder even when reasoning effort is medium/high")
	cmd.Flags().String("dirty-start-policy", "", "Dirty repo policy before starting: error, warn, or quarantine")
	cmd.Flags().String("dirty-during-policy", "", "Dirty repo policy after iterations: error, warn, or quarantine")
	cmd.Flags().Bool("allow-untracked", false, "Allow untracked files when checking for dirtiness")
	cmd.Flags().Bool("no-allow-untracked", false, "Treat untracked files as dirty for preflight checks")
	cmd.Flags().Bool("quarantine-clean-untracked", false, "Allow quarantine to delete untracked files")
	cmd.Flags().Bool("no-quarantine-clean-untracked", false, "Prevent quarantine from deleting untracked files")

	return cmd
}

func newLoopFixupCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "fixup",
		Short:   "Re-attempt blocked items with WIP metadata",
		Long:    "Scan blocked items with WIP metadata, validate in an isolated worktree, and requeue when safe.",
		Example: "  ralph loop fixup\n  ralph loop fixup --max-attempts 2 --max-items 3",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			locs, err := paths.Resolve("")
			if err != nil {
				return err
			}

			flags := cmd.Flags()
			maxAttempts, err := flags.GetInt("max-attempts")
			if err != nil {
				return err
			}
			maxItems, err := flags.GetInt("max-items")
			if err != nil {
				return err
			}
			if maxAttempts < 0 || maxItems < 0 {
				return fmt.Errorf("fixup numeric values must be non-negative")
			}

			logger := loop.StdLogger{Writer: cmd.OutOrStdout()}
			result, err := loop.FixupBlockedItems(context.Background(), loop.FixupOptions{
				RepoRoot:            locs.RepoRoot,
				PinDir:              cfg.Paths.PinDir,
				MaxAttempts:         maxAttempts,
				MaxItems:            maxItems,
				RequireMain:         cfg.Loop.RequireMain,
				AutoCommit:          cfg.Git.AutoCommit,
				AutoPush:            cfg.Git.AutoPush,
				RedactionMode:       cfg.Logging.RedactionMode,
				LogMaxBufferedBytes: cfg.Logging.MaxBufferedBytes,
				Logger:              logger,
			})
			if err != nil {
				return err
			}

			_, _ = fmt.Fprintf(cmd.OutOrStdout(), "Scanned blocked: %d\n", result.ScannedBlocked)
			_, _ = fmt.Fprintf(cmd.OutOrStdout(), "Eligible: %d\n", result.Eligible)
			if len(result.RequeuedIDs) > 0 {
				_, _ = fmt.Fprintf(cmd.OutOrStdout(), "Requeued: %s\n", strings.Join(result.RequeuedIDs, ", "))
			}
			if len(result.SkippedMax) > 0 {
				_, _ = fmt.Fprintf(cmd.OutOrStdout(), "Skipped max attempts: %s\n", strings.Join(result.SkippedMax, ", "))
			}
			if len(result.FailedIDs) > 0 {
				_, _ = fmt.Fprintf(cmd.OutOrStdout(), "Failed: %s\n", strings.Join(result.FailedIDs, ", "))
			}
			return nil
		},
	}

	cmd.Flags().Int("max-attempts", 3, "Max fixup attempts per blocked item (0 = unlimited)")
	cmd.Flags().Int("max-items", 0, "Max blocked items to process per run (0 = unlimited)")

	return cmd
}

func parseOnlyTagsCLI(value string) ([]string, error) {
	if strings.TrimSpace(value) == "" {
		return []string{}, nil
	}
	return pin.ValidateTagList("--only-tag", value)
}

func resolveFlagString(flags *pflag.FlagSet, name string, fallback string) (string, error) {
	if !flags.Changed(name) {
		return fallback, nil
	}
	return flags.GetString(name)
}

func resolveRunnerFlag(flags *pflag.FlagSet, name string, fallback string) (string, error) {
	value, err := resolveFlagString(flags, name, fallback)
	if err != nil {
		return "", err
	}
	return runnerargs.NormalizeRunner(value), nil
}

func mergeRunnerArgsWithEffort(runner string, configArgs []string, cliArgs []string, effort string) []string {
	merged := runnerargs.MergeArgs(configArgs, cliArgs)
	return runnerargs.ApplyReasoningEffort(runner, merged, effort).Args
}

func newPinCommand() *cobra.Command {
	pinCmd := &cobra.Command{
		Use:     "pin",
		Short:   "Pin queue operations",
		Long:    "Validate and manipulate Ralph pin queue files.",
		Example: "  ralph pin validate\n  ralph pin next-id\n  ralph pin fix-ids\n  ralph pin move-checked\n  ralph pin block-item --item-id RQ-0001 --reason \"needs fixes\"",
	}

	pinCmd.AddCommand(newPinValidateCommand())
	pinCmd.AddCommand(newPinNextIDCommand())
	pinCmd.AddCommand(newPinFixIDsCommand())
	pinCmd.AddCommand(newPinMoveCheckedCommand())
	pinCmd.AddCommand(newPinBlockItemCommand())

	return pinCmd
}

func newPinValidateCommand() *cobra.Command {
	return &cobra.Command{
		Use:     "validate",
		Short:   "Validate pin files",
		Long:    "Validate Ralph pin/spec files for structure and ID integrity.",
		Example: "  ralph pin validate",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			files := pin.ResolveFiles(cfg.Paths.PinDir)
			if err := pin.ValidatePin(files, cfg.ProjectType); err != nil {
				return err
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), ">> [RALPH] Pin validation OK.")
			return err
		},
	}
}

func newPinNextIDCommand() *cobra.Command {
	return &cobra.Command{
		Use:     "next-id",
		Short:   "Print the next available queue ID",
		Long:    "Scan queue and done pin files and print the next available RQ-#### ID.",
		Example: "  ralph pin next-id\n  ralph --pin-dir .ralph/pin pin next-id",
		Args:    cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			files := pin.ResolveFiles(cfg.Paths.PinDir)
			if err := pin.ValidatePin(files, cfg.ProjectType); err != nil {
				return err
			}
			nextID, err := pin.NextQueueID(files, "")
			if err != nil {
				return err
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), nextID)
			return err
		},
	}
}

func newPinFixIDsCommand() *cobra.Command {
	return &cobra.Command{
		Use:     "fix-ids",
		Short:   "Fix duplicate queue IDs",
		Long:    "Renumber duplicate queue IDs without modifying the done log.",
		Example: "  ralph pin fix-ids\n  ralph --pin-dir .ralph/pin pin fix-ids",
		Args:    cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			files := pin.ResolveFiles(cfg.Paths.PinDir)
			result, err := pin.FixDuplicateQueueIDs(files, "", cfg.ProjectType)
			if err != nil {
				return err
			}
			if len(result.Fixed) == 0 {
				_, err = fmt.Fprintln(cmd.OutOrStdout(), ">> [RALPH] No duplicate queue IDs found.")
				return err
			}
			_, err = fmt.Fprintf(cmd.OutOrStdout(), ">> [RALPH] Updated %d queue IDs:\n", len(result.Fixed))
			if err != nil {
				return err
			}
			for _, fix := range result.Fixed {
				section := ""
				if fix.Section != "" {
					section = fmt.Sprintf(" (%s)", fix.Section)
				}
				if _, err := fmt.Fprintf(cmd.OutOrStdout(), "- %s -> %s%s\n", fix.OldID, fix.NewID, section); err != nil {
					return err
				}
			}
			return nil
		},
	}
}

func newPinMoveCheckedCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "move-checked",
		Short:   "Move checked queue items into the done log",
		Long:    "Move checked queue items from the Queue section into the Done log. Defaults to prepending new Done items.",
		Example: "  ralph pin move-checked\n  ralph pin move-checked --append",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			appendMode, err := cmd.Flags().GetBool("append")
			if err != nil {
				return err
			}
			prepend := !appendMode
			files := pin.ResolveFiles(cfg.Paths.PinDir)
			ids, err := pin.MoveCheckedToDone(files.QueuePath, files.DonePath, prepend)
			if err != nil {
				return err
			}
			summary := pin.SummarizeIDs(ids)
			if summary != "" {
				_, err = fmt.Fprintln(cmd.OutOrStdout(), summary)
				return err
			}
			return nil
		},
	}
	cmd.Flags().Bool("append", false, "Append moved items to the bottom of the Done section (legacy; default prepends)")
	return cmd
}

func newPinBlockItemCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "block-item",
		Short:   "Move a queue item into Blocked with metadata",
		Long:    "Move a queue item into Blocked and append metadata about why it was blocked.",
		Example: "  ralph pin block-item --item-id RQ-0001 --reason \"make ci failed\"",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			itemID, err := cmd.Flags().GetString("item-id")
			if err != nil {
				return err
			}
			reasons, err := cmd.Flags().GetStringArray("reason")
			if err != nil {
				return err
			}
			reasonLines := make([]string, 0)
			for _, reason := range reasons {
				for _, line := range strings.Split(reason, "\n") {
					if strings.TrimSpace(line) != "" {
						reasonLines = append(reasonLines, line)
					}
				}
			}
			if len(reasonLines) == 0 {
				return fmt.Errorf("At least one --reason line is required to block an item.")
			}
			wipBranch, _ := cmd.Flags().GetString("wip-branch")
			knownGood, _ := cmd.Flags().GetString("known-good")
			unblockHint, _ := cmd.Flags().GetString("unblock-hint")

			files := pin.ResolveFiles(cfg.Paths.PinDir)
			ok, err := pin.BlockItem(files.QueuePath, itemID, reasonLines, pin.Metadata{
				WIPBranch:   wipBranch,
				KnownGood:   knownGood,
				UnblockHint: unblockHint,
			})
			if err != nil {
				return err
			}
			if !ok {
				return fmt.Errorf("Item %s not found in Queue.", itemID)
			}
			return nil
		},
	}
	cmd.Flags().String("item-id", "", "Queue item ID (e.g., RQ-0123)")
	cmd.Flags().StringArray("reason", []string{}, "Reason line (repeatable)")
	cmd.Flags().String("wip-branch", "", "WIP branch name containing quarantined work")
	cmd.Flags().String("known-good", "", "Known-good Git SHA before the failure")
	cmd.Flags().String("unblock-hint", "", "Hint describing how to unblock")
	_ = cmd.MarkFlagRequired("item-id")
	return cmd
}
