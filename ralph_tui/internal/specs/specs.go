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
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/procgroup"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/prompts"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

const (
	RunnerCodex    Runner = "codex"
	RunnerOpencode Runner = "opencode"
)

const (
	interactivePlaceholder = "{{INTERACTIVE_INSTRUCTIONS}}"
	innovatePlaceholder    = "{{INNOVATE_INSTRUCTIONS}}"
	scoutPlaceholder       = "{{SCOUT_WORKFLOW}}"
	bugSweepPlaceholder    = "{{BUG_SWEEP_ENTRY}}"
	userFocusPlaceholder   = "{{USER_FOCUS}}"
)

// FillPromptOptions controls how template placeholders are replaced.
type FillPromptOptions struct {
	Interactive   bool
	Innovate      bool
	ScoutWorkflow bool
	UserFocus     string
	ProjectType   project.Type
}

// Runner selects which specs runner to invoke.
type Runner string

// BuildOptions controls the specs builder invocation.
type BuildOptions struct {
	RepoRoot         string
	PinDir           string
	PromptTemplate   string
	ProjectType      project.Type
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

func innovateInstructionsFor(projectType project.Type) (string, error) {
	return prompts.SpecsInnovateInstructions(projectType)
}

func scoutWorkflowTemplateFor(projectType project.Type) (string, error) {
	return prompts.SpecsScoutWorkflowTemplate(projectType)
}

func scoutWorkflowInstructions(projectType project.Type, userFocus string) (string, error) {
	template, err := scoutWorkflowTemplateFor(projectType)
	if err != nil {
		return "", err
	}
	focus := strings.TrimSpace(userFocus)
	if focus == "" {
		focus = "(none provided)"
	}
	if !strings.Contains(template, userFocusPlaceholder) {
		return "", fmt.Errorf("scout workflow template missing %s placeholder", userFocusPlaceholder)
	}
	return strings.ReplaceAll(template, userFocusPlaceholder, focus), nil
}

// ResolvePromptTemplate returns the prompt template path, creating defaults when needed.
func ResolvePromptTemplate(pinDir string, projectType project.Type, promptPath string) (string, error) {
	if strings.TrimSpace(promptPath) != "" {
		return promptPath, nil
	}
	return pin.EnsureSpecsTemplate(pinDir, projectType)
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
	resolvedType, err := project.ResolveType(opts.ProjectType)
	if err != nil {
		return "", err
	}

	interactiveInstructions := ""
	if opts.Interactive {
		interactiveInstructions, err = prompts.SpecsInteractiveInstructions()
		if err != nil {
			return "", err
		}
	}
	prompt, err = replacePlaceholder(prompt, interactivePlaceholder, interactiveInstructions, opts.Interactive)
	if err != nil {
		return "", err
	}
	innovateInstructions := ""
	if opts.Innovate {
		innovateInstructions, err = innovateInstructionsFor(resolvedType)
		if err != nil {
			return "", err
		}
	}
	prompt, err = replacePlaceholder(prompt, innovatePlaceholder, innovateInstructions, opts.Innovate)
	if err != nil {
		return "", err
	}
	scoutInstructions := ""
	if opts.ScoutWorkflow {
		scoutInstructions, err = scoutWorkflowInstructions(resolvedType, opts.UserFocus)
		if err != nil {
			return "", err
		}
	}
	prompt, err = replacePlaceholder(prompt, scoutPlaceholder, scoutInstructions, opts.ScoutWorkflow)
	if err != nil {
		return "", err
	}
	prompt, err = replaceBugSweepPlaceholder(prompt, resolvedType)
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
	if innovateExplicit {
		return InnovateResolution{Effective: innovate}, nil
	}
	if !autofillScout {
		return InnovateResolution{Effective: innovate}, nil
	}
	count, err := queueTopLevelItemCount(queuePath)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			autoEnabled := !innovate
			reason := ""
			if autoEnabled {
				reason = "missing queue file"
			}
			return InnovateResolution{
				Effective:   innovate || autoEnabled,
				AutoEnabled: autoEnabled,
				AutoReason:  reason,
			}, nil
		}
		return InnovateResolution{}, err
	}
	if count == 0 {
		autoEnabled := !innovate
		reason := ""
		if autoEnabled {
			reason = "empty queue"
		}
		return InnovateResolution{
			Effective:   innovate || autoEnabled,
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
	templatePath, err := ResolvePromptTemplate(opts.PinDir, opts.ProjectType, opts.PromptTemplate)
	if err != nil {
		return BuildResult{}, err
	}
	opts.PromptTemplate = templatePath
	normalizedRunner, err := normalizeAndValidateRunner(opts.Runner)
	if err != nil {
		return BuildResult{}, err
	}
	opts.Runner = normalizedRunner

	if opts.PrintPrompt {
		prompt, effectiveInnovate, err := buildPrompt(opts)
		if err != nil {
			return BuildResult{}, err
		}
		return BuildResult{
			Prompt:            prompt,
			EffectiveInnovate: effectiveInnovate,
		}, nil
	}

	if err := verifyRunner(opts.runnerBackend(), opts.Runner); err != nil {
		return BuildResult{}, err
	}

	lock, err := AcquireLock(opts.RepoRoot)
	if err != nil {
		return BuildResult{}, err
	}
	defer lock.Release()

	prompt, effectiveInnovate, err := buildPrompt(opts)
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

func buildPrompt(opts BuildOptions) (string, bool, error) {
	effectiveInnovate, err := ResolveInnovate(filepath.Join(opts.PinDir, "implementation_queue.md"), opts.Innovate, opts.InnovateExplicit, opts.AutofillScout)
	if err != nil {
		return "", false, err
	}

	prompt, err := FillPrompt(opts.PromptTemplate, FillPromptOptions{
		Interactive:   opts.Interactive,
		Innovate:      effectiveInnovate,
		ScoutWorkflow: opts.ScoutWorkflow,
		UserFocus:     opts.UserFocus,
		ProjectType:   opts.ProjectType,
	})
	if err != nil {
		return "", false, err
	}

	return prompt, effectiveInnovate, nil
}

// AcquireLock obtains the Ralph build lock.
func AcquireLock(repoRoot string) (*Lock, error) {
	lockBase := os.Getenv("TMPDIR")
	if lockBase == "" {
		lockBase = os.TempDir()
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

func replaceBugSweepPlaceholder(content string, projectType project.Type) (string, error) {
	if !strings.Contains(content, bugSweepPlaceholder) {
		return content, nil
	}
	entry, err := prompts.BugSweepEntry(projectType)
	if err != nil {
		return "", err
	}
	return strings.ReplaceAll(content, bugSweepPlaceholder, strings.TrimSpace(entry)), nil
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

	invocation, err := runnerargs.BuildRunnerCommand(string(opts.Runner), runArgs, prompt, promptPath, opts.Interactive)
	if err != nil {
		return err
	}
	cmd := backend.CommandContext(ctx, invocation.Name, invocation.Args...)
	procgroup.Configure(cmd)
	cmd.Stdout = stdout
	cmd.Stderr = stderr
	var file *os.File
	if invocation.PromptStdinPath != "" {
		file, err = os.Open(invocation.PromptStdinPath)
		if err != nil {
			return err
		}
		defer file.Close()
		cmd.Stdin = file
	} else {
		cmd.Stdin = stdin
	}
	defer flushWriters(stdout, stderr)
	if err := cmd.Run(); err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return fmt.Errorf("%s failed while building specs: %w", invocation.Name, err)
	}
	return nil
}

type writerFlusher interface {
	Flush()
}

func flushWriters(stdout io.Writer, stderr io.Writer) {
	flushWriter(stdout)
	if stderr == nil || stderr == stdout {
		return
	}
	flushWriter(stderr)
}

func flushWriter(writer io.Writer) {
	flusher, ok := writer.(writerFlusher)
	if !ok || flusher == nil {
		return
	}
	flusher.Flush()
}

func verifyRunner(backend RunnerBackend, runner Runner) error {
	normalized, err := normalizeAndValidateRunner(runner)
	if err != nil {
		return err
	}
	switch normalized {
	case RunnerCodex:
		if _, err := backend.LookPath("codex"); err != nil {
			return fmt.Errorf("codex is not on PATH. Install it or use --runner opencode.")
		}
	case RunnerOpencode:
		if _, err := backend.LookPath("opencode"); err != nil {
			return fmt.Errorf("opencode is not on PATH. Install it or use --runner codex.")
		}
	}
	return nil
}

func normalizeAndValidateRunner(value Runner) (Runner, error) {
	normalized := Runner(runnerargs.NormalizeRunner(string(value)))
	if normalized == "" {
		normalized = RunnerCodex
	}
	switch normalized {
	case RunnerCodex, RunnerOpencode:
		return normalized, nil
	default:
		trimmed := strings.TrimSpace(string(value))
		if trimmed == "" {
			trimmed = string(value)
		}
		return "", fmt.Errorf("--runner must be codex or opencode (got: %s)", trimmed)
	}
}

func enforceInteractiveSize(prompt string, runner Runner) error {
	promptSize := len([]byte(prompt))
	if promptSize <= 200000 {
		return nil
	}
	if runnerargs.NormalizeRunner(string(runner)) == string(RunnerCodex) {
		return fmt.Errorf("Prompt too large for interactive codex (size: %d bytes). Use non-interactive or opencode.", promptSize)
	}
	return fmt.Errorf("Prompt too large for interactive opencode (size: %d bytes). Use non-interactive or codex.", promptSize)
}

func queueTopLevelItemCount(queuePath string) (int, error) {
	items, err := pin.ReadQueueItems(queuePath)
	if err != nil {
		return 0, err
	}
	return len(items), nil
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
