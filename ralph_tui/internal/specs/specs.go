// Package specs builds Ralph specs prompts and invokes runners.
// Entrypoint: Build, FillPrompt.
package specs

import (
	"context"
	"errors"
	"fmt"
	"hash/crc32"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/lockfile"
	"github.com/mitchfultz/ralph/ralph_tui/internal/procgroup"
)

const (
	RunnerCodex    Runner = "codex"
	RunnerOpencode Runner = "opencode"
)

const (
	interactivePlaceholder = "{{INTERACTIVE_INSTRUCTIONS}}"
	innovatePlaceholder    = "{{INNOVATE_INSTRUCTIONS}}"
	scoutPlaceholder       = "{{SCOUT_WORKFLOW}}"
)

const interactiveInstructions = "INTERACTIVE MODE ENABLED. Before adding any new queue items:\n" +
	"1) List the candidate items you intend to add (bulleted).\n" +
	"2) Ask the user for directives/approval or edits.\n" +
	"3) Wait for the user's response, then incorporate it.\n" +
	"If no new items are proposed, ask the user if they want any new directions.\n"

const innovateInstructions = "AUTOFILL/SCOUT MODE ENABLED (BUG-HUNT).\n" +
	"\n" +
	"This repo intentionally avoids TODO/TBD placeholders. You must rely on evidence from the repo and prioritize:\n" +
	"- architectural debt and risky coupling\n" +
	"- duplicated logic across packages (e.g., TUI vs legacy scripts)\n" +
	"- workflow gaps in Makefile or CLI flows\n" +
	"- missing regression tests for brittle paths\n" +
	"- config/state mismatches between defaults, UI, and CLI\n" +
	"\n" +
	"Mandatory scouting (repo_prompt):\n" +
	"- Start by calling get_file_tree.\n" +
	"- Then read a small but representative set of files across ralph_tui/internal/, ralph_tui/cmd/, ralph_legacy/bin/, ralph_legacy/specs/, and .ralph/pin/.\n" +
	"\n" +
	"Queue seeding rule:\n" +
	"- If `## Queue` is empty, you MUST populate it with 10-15 high-leverage, outcome-sized items.\n" +
	"\n" +
	"Evidence requirement for NEW items:\n" +
	"- Each item must cite concrete file paths and what you observed (function/class/pattern), or a concrete Make target/workflow gap.\n" +
	"- Do not invent evidence; only claim what you can point to in the repo.\n"

const scoutWorkflowTemplate = "SCOUT WORKFLOW ENABLED.\n" +
	"\n" +
	"Goal: run a focused bug hunt and seed evidence-backed queue items.\n" +
	"1) Confirm the focus area below. If it is missing or vague, interpret it conservatively.\n" +
	"2) Scan the lookup table + pin files to find related modules.\n" +
	"3) Read targeted files and identify real, concrete risks (bugs, regressions, missing tests).\n" +
	"4) Propose queue items scoped to the focus area with evidence and a clear plan.\n" +
	"5) Prefer fixes that centralize shared logic and prevent the same bug class from recurring.\n" +
	"\n" +
	"User focus prompt:\n%s\n"

// FillPromptOptions controls how template placeholders are replaced.
type FillPromptOptions struct {
	Interactive   bool
	Innovate      bool
	ScoutWorkflow bool
	UserFocus     string
}

// Runner selects which specs runner to invoke.
type Runner string

// BuildOptions controls the specs builder invocation.
type BuildOptions struct {
	RepoRoot         string
	PinDir           string
	PromptTemplate   string
	Runner           Runner
	RunnerArgs       []string
	Interactive      bool
	Innovate         bool
	InnovateExplicit bool
	AutofillScout    bool
	ScoutWorkflow    bool
	UserFocus        string
	PrintPrompt      bool
	Stdout           io.Writer
	Stderr           io.Writer
	Stdin            io.Reader
	RunnerBackend    RunnerBackend
}

// BuildResult captures build outputs.
type BuildResult struct {
	Prompt            string
	PromptPath        string
	EffectiveInnovate bool
}

// InnovateResolution captures the effective innovate state and any auto-enable reason.
type InnovateResolution struct {
	Effective   bool
	AutoEnabled bool
	AutoReason  string
}

// RunnerBackend abstracts runner execution for hermetic tests.
type RunnerBackend interface {
	LookPath(file string) (string, error)
	CommandContext(ctx context.Context, name string, args ...string) *exec.Cmd
}

type defaultRunnerBackend struct{}

func (defaultRunnerBackend) LookPath(file string) (string, error) {
	return exec.LookPath(file)
}

func (defaultRunnerBackend) CommandContext(ctx context.Context, name string, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, name, args...)
}

func (o BuildOptions) runnerBackend() RunnerBackend {
	if o.RunnerBackend != nil {
		return o.RunnerBackend
	}
	return defaultRunnerBackend{}
}

func scoutWorkflowInstructions(userFocus string) string {
	focus := strings.TrimSpace(userFocus)
	if focus == "" {
		focus = "(none provided)"
	}
	return fmt.Sprintf(scoutWorkflowTemplate, focus)
}

// FillPrompt loads and fills the prompt template with interactive/innovate/scout placeholders.
func FillPrompt(templatePath string, opts FillPromptOptions) (string, error) {
	content, err := os.ReadFile(templatePath)
	if err != nil {
		return "", err
	}
	prompt := string(content)
	if !strings.Contains(prompt, "AGENTS.md") {
		return "", fmt.Errorf("Prompt template must reference AGENTS.md (root): %s", templatePath)
	}

	prompt, err = replacePlaceholder(prompt, interactivePlaceholder, interactiveInstructions, opts.Interactive)
	if err != nil {
		return "", err
	}
	prompt, err = replacePlaceholder(prompt, innovatePlaceholder, innovateInstructions, opts.Innovate)
	if err != nil {
		return "", err
	}
	scoutInstructions := scoutWorkflowInstructions(opts.UserFocus)
	prompt, err = replacePlaceholder(prompt, scoutPlaceholder, scoutInstructions, opts.ScoutWorkflow)
	if err != nil {
		return "", err
	}

	return prompt, nil
}

// ResolveInnovate applies the autofill scout rules to determine the effective innovate mode.
func ResolveInnovate(queuePath string, innovate bool, innovateExplicit bool, autofillScout bool) (bool, error) {
	resolution, err := ResolveInnovateDetails(queuePath, innovate, innovateExplicit, autofillScout)
	if err != nil {
		return false, err
	}
	return resolution.Effective, nil
}

// ResolveInnovateDetails returns the effective innovate state plus any auto-enable reason.
func ResolveInnovateDetails(queuePath string, innovate bool, innovateExplicit bool, autofillScout bool) (InnovateResolution, error) {
	if innovateExplicit || !autofillScout {
		return InnovateResolution{Effective: innovate}, nil
	}
	count, err := uncheckedQueueCount(queuePath)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			autoEnabled := !innovateExplicit && autofillScout && !innovate
			reason := ""
			if autoEnabled {
				reason = "missing queue file"
			}
			return InnovateResolution{
				Effective:   true,
				AutoEnabled: autoEnabled,
				AutoReason:  reason,
			}, nil
		}
		return InnovateResolution{}, err
	}
	if count == 0 {
		autoEnabled := !innovateExplicit && autofillScout && !innovate
		reason := ""
		if autoEnabled {
			reason = "empty queue"
		}
		return InnovateResolution{
			Effective:   true,
			AutoEnabled: autoEnabled,
			AutoReason:  reason,
		}, nil
	}
	return InnovateResolution{Effective: innovate}, nil
}

// Build runs the specs builder with the given options.
func Build(ctx context.Context, opts BuildOptions) (BuildResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return BuildResult{}, err
	}
	if opts.PromptTemplate == "" {
		opts.PromptTemplate = filepath.Join(opts.PinDir, "specs_builder.md")
	}
	if opts.Runner == "" {
		opts.Runner = RunnerCodex
	}

	if err := verifyRunner(opts.runnerBackend(), opts.Runner); err != nil {
		return BuildResult{}, err
	}

	lock, err := AcquireLock(opts.RepoRoot)
	if err != nil {
		return BuildResult{}, err
	}
	defer lock.Release()

	effectiveInnovate, err := ResolveInnovate(filepath.Join(opts.PinDir, "implementation_queue.md"), opts.Innovate, opts.InnovateExplicit, opts.AutofillScout)
	if err != nil {
		return BuildResult{}, err
	}

	prompt, err := FillPrompt(opts.PromptTemplate, FillPromptOptions{
		Interactive:   opts.Interactive,
		Innovate:      effectiveInnovate,
		ScoutWorkflow: opts.ScoutWorkflow,
		UserFocus:     opts.UserFocus,
	})
	if err != nil {
		return BuildResult{}, err
	}

	promptPath, cleanup, err := writeTempPrompt(prompt)
	if err != nil {
		return BuildResult{}, err
	}
	defer cleanup()

	result := BuildResult{
		Prompt:            prompt,
		PromptPath:        promptPath,
		EffectiveInnovate: effectiveInnovate,
	}

	if opts.PrintPrompt {
		return result, nil
	}

	if opts.Interactive {
		if err := enforceInteractiveSize(prompt, opts.Runner); err != nil {
			return BuildResult{}, err
		}
	}

	if err := ctx.Err(); err != nil {
		return BuildResult{}, err
	}
	if err := runRunner(ctx, opts, prompt, promptPath); err != nil {
		return BuildResult{}, err
	}

	return result, nil
}

// AcquireLock obtains the Ralph build lock.
func AcquireLock(repoRoot string) (*Lock, error) {
	lockBase := os.Getenv("TMPDIR")
	if lockBase == "" {
		lockBase = "/tmp"
	}

	lockID := lockChecksum(repoRoot)
	lockDir := filepath.Join(strings.TrimRight(lockBase, string(os.PathSeparator)), fmt.Sprintf("ralph.lock.%s", lockID))
	lock, err := lockfile.Acquire(lockDir, lockfile.AcquireOptions{AllowAncestor: true})
	if err != nil {
		return nil, err
	}
	return &Lock{dir: lock.Dir(), acquired: lock.Acquired()}, nil
}

// Lock represents an acquired Ralph lock.
type Lock struct {
	dir      string
	acquired bool
}

// Release frees the lock if this process acquired it.
func (l *Lock) Release() {
	if l == nil || !l.acquired {
		return
	}
	_ = os.RemoveAll(l.dir)
}

func replacePlaceholder(content string, placeholder string, instructions string, enabled bool) (string, error) {
	replacement := ""
	if enabled {
		replacement = instructions
	}
	if strings.Contains(content, placeholder) {
		return strings.ReplaceAll(content, placeholder, replacement), nil
	}
	if enabled {
		return "", fmt.Errorf("Error: Prompt template missing %s placeholder", placeholder)
	}
	return content, nil
}

func writeTempPrompt(prompt string) (string, func(), error) {
	file, err := os.CreateTemp("", "ralph_specs_*.md")
	if err != nil {
		return "", func() {}, err
	}
	path := file.Name()
	if _, err := file.WriteString(prompt); err != nil {
		_ = file.Close()
		_ = os.Remove(path)
		return "", func() {}, err
	}
	if err := file.Close(); err != nil {
		_ = os.Remove(path)
		return "", func() {}, err
	}
	cleanup := func() { _ = os.Remove(path) }
	return path, cleanup, nil
}

func runRunner(ctx context.Context, opts BuildOptions, prompt string, promptPath string) error {
	backend := opts.runnerBackend()
	runArgs := append([]string{}, opts.RunnerArgs...)
	stdout := opts.Stdout
	stderr := opts.Stderr
	stdin := opts.Stdin
	if stdout == nil {
		stdout = os.Stdout
	}
	if stderr == nil {
		stderr = os.Stderr
	}
	if stdin == nil {
		stdin = os.Stdin
	}

	switch opts.Runner {
	case RunnerCodex:
		if opts.Interactive {
			cmd := backend.CommandContext(ctx, "codex", append(runArgs, prompt)...)
			procgroup.Configure(cmd)
			cmd.Stdout = stdout
			cmd.Stderr = stderr
			cmd.Stdin = stdin
			if err := cmd.Run(); err != nil {
				if ctx.Err() != nil {
					return ctx.Err()
				}
				return fmt.Errorf("codex failed while building specs: %w", err)
			}
			return nil
		}
		args := append([]string{"exec"}, runArgs...)
		args = append(args, "-")
		cmd := backend.CommandContext(ctx, "codex", args...)
		procgroup.Configure(cmd)
		file, err := os.Open(promptPath)
		if err != nil {
			return err
		}
		defer file.Close()
		cmd.Stdin = file
		cmd.Stdout = stdout
		cmd.Stderr = stderr
		if err := cmd.Run(); err != nil {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			return fmt.Errorf("codex failed while building specs: %w", err)
		}
		return nil
	case RunnerOpencode:
		if opts.Interactive {
			cmd := backend.CommandContext(ctx, "opencode", append(runArgs, prompt)...)
			procgroup.Configure(cmd)
			cmd.Stdout = stdout
			cmd.Stderr = stderr
			cmd.Stdin = stdin
			if err := cmd.Run(); err != nil {
				if ctx.Err() != nil {
					return ctx.Err()
				}
				return fmt.Errorf("opencode failed while building specs: %w", err)
			}
			return nil
		}
		args := append([]string{"run"}, runArgs...)
		args = append(args, "--file", promptPath, "--", "Follow the attached prompt file verbatim.")
		cmd := backend.CommandContext(ctx, "opencode", args...)
		procgroup.Configure(cmd)
		cmd.Stdout = stdout
		cmd.Stderr = stderr
		cmd.Stdin = stdin
		if err := cmd.Run(); err != nil {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			return fmt.Errorf("opencode failed while building specs: %w", err)
		}
		return nil
	default:
		return fmt.Errorf("--runner must be codex or opencode (got: %s)", opts.Runner)
	}
}

func verifyRunner(backend RunnerBackend, runner Runner) error {
	switch runner {
	case RunnerCodex:
		if _, err := backend.LookPath("codex"); err != nil {
			return fmt.Errorf("codex is not on PATH. Install it or use --runner opencode.")
		}
	case RunnerOpencode:
		if _, err := backend.LookPath("opencode"); err != nil {
			return fmt.Errorf("opencode is not on PATH. Install it or use --runner codex.")
		}
	default:
		return fmt.Errorf("--runner must be codex or opencode (got: %s)", runner)
	}
	return nil
}

func enforceInteractiveSize(prompt string, runner Runner) error {
	promptSize := len([]byte(prompt))
	if promptSize <= 200000 {
		return nil
	}
	if runner == RunnerCodex {
		return fmt.Errorf("Prompt too large for interactive codex (size: %d bytes). Use non-interactive or opencode.", promptSize)
	}
	return fmt.Errorf("Prompt too large for interactive opencode (size: %d bytes). Use non-interactive or codex.", promptSize)
}

func uncheckedQueueCount(queuePath string) (int, error) {
	content, err := os.ReadFile(queuePath)
	if err != nil {
		return 0, err
	}
	lines := strings.Split(strings.TrimSuffix(string(content), "\n"), "\n")
	inQueue := false
	count := 0
	for _, line := range lines {
		switch {
		case strings.TrimSpace(line) == "## Queue":
			inQueue = true
		case strings.HasPrefix(line, "## "):
			inQueue = false
		case inQueue:
			trimmed := strings.TrimLeft(line, " \t")
			if strings.HasPrefix(trimmed, "- [ ]") {
				count++
			}
		}
	}
	return count, nil
}

func lockChecksum(repoRoot string) string {
	return fmt.Sprintf("%x", crc32.ChecksumIEEE([]byte(repoRoot)))
}

// GitDiffStat returns git diff --stat output for the repo.
func GitDiffStat(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "diff", "--stat")
	output, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(output)), nil
}
