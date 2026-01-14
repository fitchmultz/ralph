// Command ralph is the entrypoint for the Ralph CLI and TUI.
// Entrypoint: main.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/loop"
	"github.com/mitchfultz/ralph/ralph_tui/internal/migrate"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
	"github.com/mitchfultz/ralph/ralph_tui/internal/tui"
	"github.com/spf13/cobra"
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
			return tui.Start(cfg, locs)
		},
	}

	cmd.PersistentFlags().String("ui-theme", "", "UI theme name")
	cmd.PersistentFlags().Int("refresh-seconds", 0, "UI refresh interval in seconds")
	cmd.PersistentFlags().String("log-level", "", "Log level (debug, info, warn, error)")
	cmd.PersistentFlags().String("data-dir", "", "Data directory path")
	cmd.PersistentFlags().String("cache-dir", "", "Cache directory path")
	cmd.PersistentFlags().String("pin-dir", "", "Pin directory path")

	cmd.AddCommand(newConfigCommand())
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

			runner, err := cmd.Flags().GetString("runner")
			if err != nil {
				return err
			}
			interactive, err := cmd.Flags().GetBool("interactive")
			if err != nil {
				return err
			}
			innovate, err := cmd.Flags().GetBool("innovate")
			if err != nil {
				return err
			}
			printPrompt, err := cmd.Flags().GetBool("print-prompt")
			if err != nil {
				return err
			}
			autofillScoutFlag, err := cmd.Flags().GetBool("autofill-scout")
			if err != nil {
				return err
			}
			noAutofillScoutFlag, err := cmd.Flags().GetBool("no-autofill-scout")
			if err != nil {
				return err
			}
			if autofillScoutFlag && noAutofillScoutFlag {
				return fmt.Errorf("cannot set both --autofill-scout and --no-autofill-scout")
			}
			autofillScout := cfg.Specs.AutofillScout
			if autofillScoutFlag {
				autofillScout = true
			}
			if noAutofillScoutFlag {
				autofillScout = false
			}

			promptPath, err := cmd.Flags().GetString("prompt")
			if err != nil {
				return err
			}

			result, err := specs.Build(specs.BuildOptions{
				RepoRoot:         locs.RepoRoot,
				PinDir:           cfg.Paths.PinDir,
				PromptTemplate:   promptPath,
				Runner:           specs.Runner(runner),
				RunnerArgs:       cmd.Flags().Args(),
				Interactive:      interactive,
				Innovate:         innovate,
				InnovateExplicit: cmd.Flags().Changed("innovate"),
				AutofillScout:    autofillScout,
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
			if err := pin.ValidatePin(files); err != nil {
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
	cmd.Flags().Bool("print-prompt", false, "Print the filled prompt and exit")
	cmd.Flags().String("prompt", "", "Prompt template path (default: .ralph/pin/specs_builder.md)")

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

	return loopCmd
}

func newLoopRunCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "run",
		Short:   "Run the supervised loop",
		Long:    "Run the Ralph worker loop, enforcing pin invariants and quarantine rules.",
		Example: "  ralph loop run --once\n  ralph loop run --only-tag db,ui",
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

			runnerName := "codex"
			if flags.Changed("runner") {
				value, err := flags.GetString("runner")
				if err != nil {
					return err
				}
				runnerName = value
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

			onlyTags := cfg.Loop.OnlyTags
			if flags.Changed("only-tag") {
				value, err := flags.GetString("only-tag")
				if err != nil {
					return err
				}
				onlyTags = value
			}

			runOnce, _ := flags.GetBool("once")

			if sleepSeconds < 0 || maxIterations < 0 || maxStalled < 0 || maxRepair < 0 {
				return fmt.Errorf("loop numeric values must be non-negative")
			}

			logger := loop.StdLogger{Writer: cmd.OutOrStdout()}
			runner, err := loop.NewRunner(loop.Options{
				RepoRoot:          locs.RepoRoot,
				PinDir:            cfg.Paths.PinDir,
				PromptPath:        promptPath,
				SupervisorPrompt:  supervisorPrompt,
				Runner:            runnerName,
				RunnerArgs:        cmd.Flags().Args(),
				SleepSeconds:      sleepSeconds,
				MaxIterations:     maxIterations,
				MaxStalled:        maxStalled,
				MaxRepairAttempts: maxRepair,
				OnlyTags:          splitTagsCLI(onlyTags, ""),
				Once:              runOnce,
				RequireMain:       cfg.Loop.RequireMain,
				AutoCommit:        cfg.Git.AutoCommit,
				AutoPush:          cfg.Git.AutoPush,
				Logger:            logger,
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
	cmd.Flags().String("only-tag", "", "Only execute queue items tagged with [tag] (comma-separated)")
	cmd.Flags().Bool("once", false, "Run exactly one iteration and exit")

	return cmd
}

func splitTagsCLI(flag string, fallback string) []string {
	value := strings.TrimSpace(flag)
	if value == "" {
		value = strings.TrimSpace(fallback)
	}
	if value == "" {
		return []string{}
	}
	parts := strings.Split(value, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		trimmed := strings.TrimSpace(part)
		if trimmed != "" {
			out = append(out, trimmed)
		}
	}
	return out
}

func newPinCommand() *cobra.Command {
	pinCmd := &cobra.Command{
		Use:     "pin",
		Short:   "Pin queue operations",
		Long:    "Validate and manipulate Ralph pin queue files.",
		Example: "  ralph pin validate\n  ralph pin move-checked\n  ralph pin block-item --item-id RQ-0001 --reason \"needs fixes\"",
	}

	pinCmd.AddCommand(newPinValidateCommand())
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
			if err := pin.ValidatePin(files); err != nil {
				return err
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), ">> [RALPH] Pin validation OK.")
			return err
		},
	}
}

func newPinMoveCheckedCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "move-checked",
		Short:   "Move checked queue items into the done log",
		Long:    "Move checked queue items from the Queue section into the Done log.",
		Example: "  ralph pin move-checked\n  ralph pin move-checked --prepend",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := loadConfig(cmd)
			if err != nil {
				return err
			}
			prepend, err := cmd.Flags().GetBool("prepend")
			if err != nil {
				return err
			}
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
	cmd.Flags().Bool("prepend", false, "Insert moved items at the top of the Done section")
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
