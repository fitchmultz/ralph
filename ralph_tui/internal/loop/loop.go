// Package loop runs the supervised Ralph worker loop.
// Entrypoint: Runner.Run.
package loop

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/prompts"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
)

// Options controls loop execution.
type Options struct {
	RepoRoot            string
	PinDir              string
	PromptPath          string
	SupervisorPrompt    string
	Runner              string
	RunnerArgs          []string
	ReasoningEffort     string
	ForceContextBuilder bool
	SleepSeconds        int
	MaxIterations       int
	MaxStalled          int
	MaxRepairAttempts   int
	OnlyTags            []string
	Once                bool
	RequireMain         bool
	AutoCommit          bool
	AutoPush            bool
	DirtyRepoStart      DirtyRepoPolicy
	DirtyRepoDuring     DirtyRepoPolicy
	AllowUntracked      bool
	QuarantineClean     bool
	RedactionMode       redaction.Mode
	Logger              Logger
	StateSink           StateSink
}

// Runner executes the loop.
type Runner struct {
	opts                    Options
	redactor                *Redactor
	pinFiles                pin.Files
	pushFailed              bool
	pushCanceled            bool
	lastValidateOutput      string
	lastCIOutput            string
	lastFailureStage        string
	lastFailureMessage      string
	currentRunArgs          []string
	currentItemBlock        string
	effectiveEffort         string
	contextBuilderMandatory bool
	state                   State
}

const (
	makeCITimeout         = 20 * time.Minute
	cancelFinalizeTimeout = 2 * time.Minute
	gitCommitTimeout      = 2 * time.Minute
	gitPushTimeout        = 5 * time.Minute
)

type FinalizeMode int

const (
	FinalizeModeNormal FinalizeMode = iota
	FinalizeModeCancelBestEffort
)

type FinalizeOptions struct {
	Mode       FinalizeMode
	AllowPush  bool
	SkipVerify bool
}

// NewRunner constructs a loop runner.
func NewRunner(opts Options) (*Runner, error) {
	if opts.RepoRoot == "" {
		return nil, fmt.Errorf("repo root required")
	}
	if opts.PinDir == "" {
		return nil, fmt.Errorf("pin dir required")
	}
	if opts.DirtyRepoStart == "" {
		opts.DirtyRepoStart = DirtyRepoPolicyError
	}
	if opts.DirtyRepoDuring == "" {
		opts.DirtyRepoDuring = DirtyRepoPolicyQuarantine
	}
	if _, err := ParseDirtyRepoPolicy(string(opts.DirtyRepoStart)); err != nil {
		return nil, err
	}
	if _, err := ParseDirtyRepoPolicy(string(opts.DirtyRepoDuring)); err != nil {
		return nil, err
	}
	r := &Runner{
		opts: opts,
		redactor: NewRedactor(
			os.Environ(),
			redaction.CoerceMode(string(opts.RedactionMode)),
		),
		pinFiles: pin.ResolveFiles(opts.PinDir),
	}

	return r, nil
}

// Run executes the loop until completion or context cancellation.
func (r *Runner) Run(ctx context.Context) error {
	if err := r.verifyRunner(); err != nil {
		return err
	}
	if err := r.verifyFiles(); err != nil {
		return err
	}
	if err := r.verifyBranch(ctx); err != nil {
		return err
	}

	lock, err := specs.AcquireLock(r.opts.RepoRoot)
	if err != nil {
		return err
	}
	defer lock.Release()

	iterations := 0
	stalled := 0
	runMode := ModeContinuous
	if r.opts.Once {
		runMode = ModeOnce
	}
	r.lastFailureStage = ""
	r.lastFailureMessage = ""
	r.publishState(r.stateWithFailure(State{Mode: runMode, Iteration: 0}))
	defer r.publishState(r.stateWithFailure(State{Mode: ModeIdle}))

	for {
		select {
		case <-ctx.Done():
			r.finalizeOnCancel(ctx)
			return nil
		default:
		}

		firstItem, err := FirstUncheckedItem(r.pinFiles.QueuePath, r.opts.OnlyTags)
		if err != nil {
			return err
		}
		if firstItem == nil {
			if len(r.opts.OnlyTags) > 0 {
				r.logf(">> [RALPH] No unchecked items found in Queue for tags: %s. Exiting cleanly.", strings.Join(r.opts.OnlyTags, ","))
			} else {
				r.logf(">> [RALPH] No unchecked items found in Queue. Exiting cleanly.")
			}
			r.publishState(r.stateWithFailure(State{Mode: ModeIdle}))
			r.logPushFailed()
			return nil
		}

		itemID := firstItem.ID
		if itemID == "" {
			return fmt.Errorf("Queue item is missing an ID prefix (expected something like ABC-0123).")
		}

		effort := "high"
		if strings.Contains(firstItem.Header, "[P1]") {
			effort = "high"
		}
		effectiveSetting := effort
		if runnerargs.NormalizeEffort(r.opts.ReasoningEffort) != "" {
			effectiveSetting = r.opts.ReasoningEffort
		}
		r.currentRunArgs = append([]string{}, r.opts.RunnerArgs...)
		effectiveEffort := ""
		if r.opts.Runner == "codex" {
			if detected, ok := runnerargs.DetectEffort(r.currentRunArgs); ok {
				effectiveEffort = detected
			} else if runnerargs.NormalizeEffort(effectiveSetting) == "auto" {
				effectiveEffort = effort
			} else {
				effectiveEffort = runnerargs.NormalizeEffort(effectiveSetting)
			}
		}
		r.currentRunArgs = runnerargs.ApplyReasoningEffort(r.opts.Runner, r.currentRunArgs, effectiveSetting).Args
		r.effectiveEffort = effectiveEffort
		r.contextBuilderMandatory = effectiveEffort == "low" || effectiveEffort == "off" || r.opts.ForceContextBuilder

		headBefore, err := HeadSHA(ctx, r.opts.RepoRoot)
		if err != nil {
			if isCancellation(ctx, err) {
				r.finalizeOnCancel(ctx)
				return nil
			}
			logGitError(r.redactor, r.opts.Logger, "head sha", err)
			return err
		}

		block, err := CurrentItemBlock(r.pinFiles.QueuePath, itemID)
		if err != nil {
			return err
		}
		r.currentItemBlock = block

		if err := r.reconcileCheckedQueueItems(ctx); err != nil {
			if r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "pin-ops", failureMessage("Failed to move checked queue items", err), err) {
				return nil
			}
			continue
		}

		headBefore, err = HeadSHA(ctx, r.opts.RepoRoot)
		if err != nil {
			if isCancellation(ctx, err) {
				r.finalizeOnCancel(ctx)
				return nil
			}
			logGitError(r.redactor, r.opts.Logger, "head sha", err)
			return err
		}

		status, err := StatusDetails(ctx, r.opts.RepoRoot)
		if err != nil {
			if isCancellation(ctx, err) {
				r.finalizeOnCancel(ctx)
				return nil
			}
			logGitError(r.redactor, r.opts.Logger, "status", err)
			return err
		}
		if r.opts.AutoCommit {
			committed, err := AutoCommitPinOnlyChanges(ctx, r.opts.RepoRoot, r.pinFiles, "chore: commit pin changes (pre-loop)")
			if err != nil {
				logGitError(r.redactor, r.opts.Logger, "commit pin changes", err)
				return err
			}
			if committed {
				if r.opts.AutoPush {
					r.pushIfAhead(ctx)
				}
				headBefore, err = HeadSHA(ctx, r.opts.RepoRoot)
				if err != nil {
					if isCancellation(ctx, err) {
						r.finalizeOnCancel(ctx)
						return nil
					}
					logGitError(r.redactor, r.opts.Logger, "head sha", err)
					return err
				}
				status, err = StatusDetails(ctx, r.opts.RepoRoot)
				if err != nil {
					if isCancellation(ctx, err) {
						r.finalizeOnCancel(ctx)
						return nil
					}
					logGitError(r.redactor, r.opts.Logger, "status", err)
					return err
				}
			}
		}
		if !status.IsClean(r.opts.AllowUntracked) {
			switch r.opts.DirtyRepoStart {
			case DirtyRepoPolicyWarn:
				summary, err := StatusSummary(ctx, r.opts.RepoRoot)
				if err != nil {
					logGitError(r.redactor, r.opts.Logger, "status summary", err)
				}
				r.logf(">> [RALPH] Warning: working tree is dirty before iteration %d.", iterations)
				if summary != "" {
					r.logf(">> [RALPH] %s", summary)
				}
			case DirtyRepoPolicyQuarantine:
				wipBranch, err := r.quarantine(ctx, itemID, headBefore, fmt.Sprintf("Dirty repo before iteration %d.", iterations))
				if err != nil {
					r.logf("Error: %s", err.Error())
					return err
				}
				r.logf(">> [RALPH] Quarantined dirty preflight changes to %s.", wipBranch)
				continue
			default:
				return &DirtyRepoError{
					RepoRoot:       r.opts.RepoRoot,
					Stage:          "preflight",
					AllowUntracked: r.opts.AllowUntracked,
					Status:         status,
				}
			}
		}

		if err := r.runValidatePin(); err != nil {
			if r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "pin-validate", fmt.Sprintf("validate_pin failed before iteration %d.", iterations), err) {
				return nil
			}
			continue
		}

		iterations++
		r.publishState(r.stateWithFailure(State{
			Mode:            runMode,
			Iteration:       iterations,
			ActiveItemID:    itemID,
			ActiveItemTitle: ExtractItemTitle(firstItem.Header),
		}))
		r.logf(">> [RALPH] Iteration %d", iterations)

		promptFile, cleanup, err := r.writePromptFile(firstItem.Header)
		if err != nil {
			return err
		}

		runner := RunnerInvoker{
			Runner:     r.opts.Runner,
			RunnerArgs: r.currentRunArgs,
			Redactor:   r.redactor,
			Logger:     r.opts.Logger,
		}
		if err := runner.RunPrompt(ctx, promptFile); err != nil {
			cleanup()
			if r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "runner", fmt.Sprintf("%s failed on iteration %d.", r.opts.Runner, iterations), err) {
				return nil
			}
			continue
		}
		cleanup()

		if err := r.finalizeIteration(ctx, itemID, firstItem.Header, headBefore, FinalizeOptions{Mode: FinalizeModeNormal, AllowPush: true}); err != nil {
			if r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, r.lastFailureStage, r.lastFailureMessage, err) {
				return nil
			}
			continue
		}
		if ctx.Err() != nil {
			r.finalizeOnCancel(ctx)
			return nil
		}

		status, err = StatusDetails(ctx, r.opts.RepoRoot)
		if err != nil {
			if isCancellation(ctx, err) {
				r.finalizeOnCancel(ctx)
				return nil
			}
			logGitError(r.redactor, r.opts.Logger, "status", err)
			return err
		}
		if !status.IsClean(r.opts.AllowUntracked) {
			if !r.opts.AutoCommit && r.opts.DirtyRepoDuring != DirtyRepoPolicyWarn {
				return &DirtyRepoError{
					RepoRoot:       r.opts.RepoRoot,
					Stage:          "post-commit",
					AllowUntracked: r.opts.AllowUntracked,
					Status:         status,
				}
			}
			switch r.opts.DirtyRepoDuring {
			case DirtyRepoPolicyWarn:
				summary, err := StatusSummary(ctx, r.opts.RepoRoot)
				if err != nil {
					logGitError(r.redactor, r.opts.Logger, "status summary", err)
				}
				r.logf(">> [RALPH] Warning: working tree is dirty after iteration %d.", iterations)
				if summary != "" {
					r.logf(">> [RALPH] %s", summary)
				}
			case DirtyRepoPolicyQuarantine:
				if r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "post-commit", fmt.Sprintf("Working tree is dirty after iteration %d.", iterations), nil) {
					return nil
				}
				continue
			default:
				return &DirtyRepoError{
					RepoRoot:       r.opts.RepoRoot,
					Stage:          "post-commit",
					AllowUntracked: r.opts.AllowUntracked,
					Status:         status,
				}
			}
		}

		r.cleanupIterationArtifacts()

		firstAfter, err := FirstUncheckedItem(r.pinFiles.QueuePath, r.opts.OnlyTags)
		if err != nil {
			return err
		}
		headAfter, err := HeadSHA(ctx, r.opts.RepoRoot)
		if err != nil {
			if isCancellation(ctx, err) {
				r.finalizeOnCancel(ctx)
				return nil
			}
			logGitError(r.redactor, r.opts.Logger, "head sha", err)
			return err
		}
		if headBefore == headAfter && firstAfter != nil && firstAfter.Header == firstItem.Header {
			stalled++
		} else {
			stalled = 0
		}

		if r.opts.MaxStalled > 0 && stalled >= r.opts.MaxStalled {
			if r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "stall", fmt.Sprintf("Stalled for %d iterations (head and first queue item unchanged).", stalled), nil) {
				return nil
			}
			continue
		}

		if r.opts.Once {
			break
		}
		if r.opts.MaxIterations > 0 && iterations >= r.opts.MaxIterations {
			break
		}
		if r.opts.SleepSeconds > 0 {
			select {
			case <-time.After(time.Duration(r.opts.SleepSeconds) * time.Second):
			case <-ctx.Done():
				r.finalizeOnCancel(ctx)
				return nil
			}
		}
	}

	r.logPushFailed()
	return nil
}

func (r *Runner) verifyRunner() error {
	switch r.opts.Runner {
	case "codex":
		if r.opts.Runner == "codex" && os.Getenv("RALPH_LOOP_SKIP_RUNNER_CHECK") == "1" {
			return nil
		}
		if _, err := exec.LookPath("codex"); err != nil {
			return fmt.Errorf("codex is not on PATH. Install it or use --runner opencode.")
		}
	case "opencode":
		if r.opts.Runner == "opencode" && os.Getenv("RALPH_LOOP_SKIP_RUNNER_CHECK") == "1" {
			return nil
		}
		if _, err := exec.LookPath("opencode"); err != nil {
			return fmt.Errorf("opencode is not on PATH. Install it or use --runner codex.")
		}
	default:
		return fmt.Errorf("--runner must be codex or opencode (got: %s)", r.opts.Runner)
	}
	return nil
}

func (r *Runner) verifyFiles() error {
	if r.opts.PromptPath != "" {
		if _, err := os.Stat(r.opts.PromptPath); err != nil {
			return fmt.Errorf("Prompt file not found: %s", r.opts.PromptPath)
		}
	}
	if r.opts.SupervisorPrompt != "" {
		if _, err := os.Stat(r.opts.SupervisorPrompt); err != nil {
			return fmt.Errorf("Supervisor prompt file not found: %s", r.opts.SupervisorPrompt)
		}
	}
	if _, err := os.Stat(r.pinFiles.QueuePath); err != nil {
		return fmt.Errorf("Implementation queue not found: %s", r.pinFiles.QueuePath)
	}
	if _, err := os.Stat(r.pinFiles.DonePath); err != nil {
		return fmt.Errorf("Implementation done log not found: %s", r.pinFiles.DonePath)
	}
	return nil
}

func (r *Runner) verifyBranch(ctx context.Context) error {
	if !r.opts.RequireMain {
		return nil
	}
	branch, err := CurrentBranch(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "current branch", err)
		return err
	}
	if branch != "main" {
		return fmt.Errorf("Ralph loop must run on main (current: %s).", branch)
	}
	return nil
}

func (r *Runner) writePromptFile(itemLine string) (string, func(), error) {
	content, err := r.loadWorkerPrompt()
	if err != nil {
		return "", func() {}, err
	}
	builder := strings.Builder{}
	builder.WriteString(content)
	builder.WriteString("\n\n")
	if r.opts.Runner == "codex" && r.effectiveEffort != "" {
		builder.WriteString(contextBuilderPolicyBlock(r.effectiveEffort, r.contextBuilderMandatory, r.opts.ForceContextBuilder))
		builder.WriteString("\n\n")
	}
	builder.WriteString("# CURRENT QUEUE ITEM\n")
	builder.WriteString(r.currentItemBlock)
	builder.WriteString("\n")

	file, err := os.CreateTemp("", "ralph_loop_prompt_*.md")
	if err != nil {
		return "", func() {}, err
	}
	path := file.Name()
	if _, err := file.WriteString(builder.String()); err != nil {
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

func (r *Runner) loadWorkerPrompt() (string, error) {
	if r.opts.PromptPath != "" {
		content, err := os.ReadFile(r.opts.PromptPath)
		if err != nil {
			return "", err
		}
		return string(content), nil
	}

	return prompts.WorkerPrompt(prompts.Runner(r.opts.Runner))
}

func (r *Runner) finalizeIteration(ctx context.Context, itemID string, itemLine string, headBefore string, opts FinalizeOptions) error {
	r.lastFailureStage = ""
	r.lastFailureMessage = ""

	headNow, err := HeadSHA(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "head sha", err)
		return err
	}
	if headNow != headBefore {
		r.lastFailureStage = "mechanical"
		r.lastFailureMessage = "Commit detected before controller finalize."
		return errors.New(r.lastFailureMessage)
	}

	movedIDs, err := pin.MoveCheckedToDone(r.pinFiles.QueuePath, r.pinFiles.DonePath, true)
	if err != nil {
		r.lastFailureStage = "pin-ops"
		r.lastFailureMessage = "Failed to move checked queue items."
		return err
	}
	_ = movedIDs

	firstAfter, err := FirstUncheckedItem(r.pinFiles.QueuePath, r.opts.OnlyTags)
	if err != nil {
		return err
	}
	completed := false
	if firstAfter == nil || firstAfter.ID != itemID {
		completed = true
	}

	status, err := StatusDetails(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "status", err)
		return err
	}

	if !completed {
		if !status.IsClean(r.opts.AllowUntracked) {
			r.lastFailureStage = "incomplete"
			r.lastFailureMessage = fmt.Sprintf("Working tree changed but %s was not marked complete.", itemID)
			return errors.New(r.lastFailureMessage)
		}
		if err := r.runValidatePin(); err != nil {
			r.lastFailureStage = "pin-validate"
			r.lastFailureMessage = "validate_pin failed after iteration."
			return err
		}
		return nil
	}

	if status.IsClean(r.opts.AllowUntracked) {
		r.lastFailureStage = "complete"
		r.lastFailureMessage = "Queue head moved but no changes detected."
		return errors.New(r.lastFailureMessage)
	}

	onlySpecs := true
	pinPrefix := pinPathPrefix(r.opts.RepoRoot, r.opts.PinDir)
	changed, err := DiffNameOnly(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "diff --name-only", err)
		r.lastFailureStage = "git"
		r.lastFailureMessage = "git diff failed."
		return err
	}
	for _, path := range changed {
		if !strings.HasPrefix(path, pinPrefix) {
			onlySpecs = false
			break
		}
	}

	if !onlySpecs && !opts.SkipVerify {
		if err := r.runMakeCI(ctx); err != nil {
			r.lastFailureStage = "verify"
			r.lastFailureMessage = "make ci failed."
			return err
		}
	}

	title := ExtractItemTitle(itemLine)
	if title == "" {
		title = "completed"
	}
	if r.opts.AutoCommit {
		commitCtx, cancel := withTimeout(ctx, gitCommitTimeout)
		defer cancel()
		if err := CommitAll(commitCtx, r.opts.RepoRoot, fmt.Sprintf("%s: %s", itemID, title)); err != nil {
			logGitError(r.redactor, r.opts.Logger, "commit", err)
			r.lastFailureStage = "commit"
			r.lastFailureMessage = fmt.Sprintf("git commit failed: %v", err)
			return err
		}
		if r.opts.AutoPush && opts.AllowPush {
			r.pushIfAhead(ctx)
		}
	}

	if err := r.runValidatePin(); err != nil {
		r.lastFailureStage = "pin-validate"
		r.lastFailureMessage = "validate_pin failed after commit."
		return err
	}

	return nil
}

func (r *Runner) reconcileCheckedQueueItems(ctx context.Context) error {
	movedIDs, err := pin.MoveCheckedToDone(r.pinFiles.QueuePath, r.pinFiles.DonePath, true)
	if err != nil {
		return err
	}
	summary := pin.SummarizeIDs(movedIDs)
	if summary != "" {
		status, err := StatusDetails(ctx, r.opts.RepoRoot)
		if err != nil {
			logGitError(r.redactor, r.opts.Logger, "status", err)
			return err
		}
		if status.HasTrackedChanges() && r.opts.AutoCommit {
			commitCtx, cancel := withTimeout(ctx, gitCommitTimeout)
			defer cancel()
			if err := CommitPaths(commitCtx, r.opts.RepoRoot, fmt.Sprintf("chore: move completed queue items (%s)", summary), r.pinFiles.QueuePath, r.pinFiles.DonePath); err != nil {
				logGitError(r.redactor, r.opts.Logger, "commit", err)
				return err
			}
			if r.opts.AutoPush {
				r.pushIfAhead(ctx)
			}
		}
	}
	return nil
}

func (r *Runner) runValidatePin() error {
	file, err := os.CreateTemp("", "ralph_validate_pin_*.log")
	if err != nil {
		return err
	}
	r.lastValidateOutput = file.Name()
	_ = file.Close()

	err = pin.ValidatePin(r.pinFiles)
	if err != nil {
		_ = os.WriteFile(r.lastValidateOutput, []byte(err.Error()), 0o600)
		return err
	}
	_ = os.WriteFile(r.lastValidateOutput, []byte(">> [RALPH] Pin validation OK."), 0o600)
	return nil
}

func (r *Runner) runMakeCI(ctx context.Context) error {
	file, err := os.CreateTemp("", "ralph_make_ci_*.log")
	if err != nil {
		return err
	}
	r.lastCIOutput = file.Name()
	_ = file.Close()

	ciCtx, cancel := withTimeout(ctx, makeCITimeout)
	defer cancel()
	cmd := exec.CommandContext(ciCtx, "make", "-C", r.opts.RepoRoot, "ci")
	if err := RunCommandWithFile(ciCtx, cmd, r.redactor, r.opts.Logger, r.lastCIOutput); err != nil {
		return err
	}
	return nil
}

func (r *Runner) cleanupIterationArtifacts() {
	for _, path := range []string{r.lastValidateOutput, r.lastCIOutput} {
		if path == "" {
			continue
		}
		_ = os.Remove(path)
	}
	r.lastValidateOutput = ""
	r.lastCIOutput = ""
}

func (r *Runner) finalizeOnCancel(ctx context.Context) {
	r.logf(">> [RALPH] Cancellation detected; attempting best-effort reconciliation.")
	cleanupCtx, cancel := withTimeout(context.Background(), cancelFinalizeTimeout)
	defer cancel()

	if err := r.reconcileCheckedQueueItems(cleanupCtx); err != nil {
		r.logf(">> [RALPH] Warning: failed to reconcile checked queue items on cancel: %v", err)
	}
	if err := r.runValidatePin(); err != nil {
		r.logf(">> [RALPH] Warning: validate_pin failed during cancel cleanup: %v", err)
	}
	r.cleanupIterationArtifacts()
}

func (r *Runner) finalizeCanceledIteration(ctx context.Context, itemID string, itemLine string, headBefore string, stage string, cause error) {
	r.logf(">> [RALPH] Cancellation during %s; skipping supervisor/quarantine.", stage)
	cleanupCtx, cancel := withTimeout(context.Background(), cancelFinalizeTimeout)
	defer cancel()

	if err := r.finalizeIteration(cleanupCtx, itemID, itemLine, headBefore, FinalizeOptions{
		Mode:       FinalizeModeCancelBestEffort,
		AllowPush:  r.opts.AutoPush,
		SkipVerify: true,
	}); err != nil {
		r.logf(">> [RALPH] Warning: cancel finalization failed: %v", err)
	}
	r.cleanupIterationArtifacts()
}

func (r *Runner) handleIterationFailure(ctx context.Context, itemID string, itemLine string, headBefore string, stage string, message string, cause error) bool {
	r.logf(">> [RALPH] Iteration failure (%s): %s", stage, message)
	if stage != "" || message != "" {
		r.recordFailure(stage, message)
	}
	if isCancellation(ctx, cause) {
		r.finalizeCanceledIteration(ctx, itemID, itemLine, headBefore, stage, cause)
		return true
	}

	attempt := 1
	for attempt <= r.opts.MaxRepairAttempts {
		r.logf(">> [RALPH] Supervisor attempt %d/%d...", attempt, r.opts.MaxRepairAttempts)
		r.runSupervisor(ctx, stage, message)
		if r.lastFailureStage == "" {
			if err := r.finalizeIteration(ctx, itemID, itemLine, headBefore, FinalizeOptions{Mode: FinalizeModeNormal, AllowPush: true}); err == nil {
				r.cleanupIterationArtifacts()
				return false
			}
			stage = r.lastFailureStage
			message = r.lastFailureMessage
		} else {
			stage = "supervisor"
			message = "Supervisor runner failed."
		}
		attempt++
	}

	wipBranch, err := r.quarantine(ctx, itemID, headBefore, message)
	if err != nil {
		r.logf("Error: %s", err.Error())
		return false
	}
	r.autoBlock(ctx, itemID, message, wipBranch, headBefore)
	r.cleanupIterationArtifacts()
	return false
}

func (r *Runner) runSupervisor(ctx context.Context, stage string, message string) {
	contextFile, cleanup, err := r.buildSupervisorContext(ctx, stage, message)
	if err != nil {
		r.lastFailureStage = "supervisor"
		r.lastFailureMessage = err.Error()
		return
	}
	defer cleanup()

	runner := RunnerInvoker{
		Runner:     r.opts.Runner,
		RunnerArgs: r.currentRunArgs,
		Redactor:   r.redactor,
		Logger:     r.opts.Logger,
	}
	if err := runner.RunPrompt(ctx, contextFile); err != nil {
		r.lastFailureStage = "supervisor"
		r.lastFailureMessage = "Supervisor runner failed."
		return
	}
	r.lastFailureStage = ""
	r.lastFailureMessage = ""
}

func (r *Runner) buildSupervisorContext(ctx context.Context, stage string, message string) (string, func(), error) {
	content, err := r.loadSupervisorPrompt()
	if err != nil {
		return "", func() {}, err
	}

	builder := strings.Builder{}
	builder.WriteString(content)
	builder.WriteString("\n\n")
	if r.opts.Runner == "codex" && r.effectiveEffort != "" {
		builder.WriteString(contextBuilderPolicyBlock(r.effectiveEffort, r.contextBuilderMandatory, r.opts.ForceContextBuilder))
		builder.WriteString("\n\n")
	}
	builder.WriteString("# FAILURE CONTEXT\n")
	builder.WriteString(fmt.Sprintf("Stage: %s\n", stage))
	builder.WriteString(fmt.Sprintf("Message: %s\n\n", message))
	builder.WriteString("# CURRENT QUEUE ITEM\n")
	builder.WriteString(r.currentItemBlock)
	builder.WriteString("\n\n")

	status, err := StatusSummary(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "status summary", err)
		status = fmt.Sprintf("Error: %v", err)
	}
	builder.WriteString("# GIT STATUS\n")
	builder.WriteString(status)
	builder.WriteString("\n\n")

	stat, err := DiffStat(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "diff --stat", err)
		stat = fmt.Sprintf("Error: %v", err)
	}
	builder.WriteString("# GIT DIFF --STAT\n")
	builder.WriteString(stat)
	builder.WriteString("\n\n")

	diff, err := Diff(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "diff", err)
		diff = fmt.Sprintf("Error: %v", err)
	}
	builder.WriteString("# GIT DIFF (truncated)\n")
	builder.WriteString(StringTail(diff, 400))
	builder.WriteString("\n")

	if r.lastValidateOutput != "" {
		if data, err := os.ReadFile(r.lastValidateOutput); err == nil {
			builder.WriteString("\n# VALIDATE PIN OUTPUT (tail)\n")
			builder.WriteString(StringTail(string(data), 200))
			builder.WriteString("\n")
		}
	}
	if r.lastCIOutput != "" {
		if data, err := os.ReadFile(r.lastCIOutput); err == nil {
			builder.WriteString("\n# MAKE CI OUTPUT (tail)\n")
			builder.WriteString(StringTail(string(data), 200))
			builder.WriteString("\n")
		}
	}

	file, err := os.CreateTemp("", "ralph_supervisor_*.md")
	if err != nil {
		return "", func() {}, err
	}
	path := file.Name()
	if _, err := file.WriteString(builder.String()); err != nil {
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

func (r *Runner) loadSupervisorPrompt() (string, error) {
	if r.opts.SupervisorPrompt != "" {
		content, err := os.ReadFile(r.opts.SupervisorPrompt)
		if err != nil {
			return "", err
		}
		return string(content), nil
	}

	return prompts.SupervisorPrompt()
}

func (r *Runner) quarantine(ctx context.Context, itemID string, headBefore string, reason string) (string, error) {
	wipBranch := CreateWipBranchName(itemID, time.Now().Format("20060102_150405"))
	candidate := wipBranch
	for attempt := 0; attempt < 5; attempt++ {
		if err := CheckoutNewBranch(ctx, r.opts.RepoRoot, candidate); err == nil {
			wipBranch = candidate
			break
		}
		candidate = fmt.Sprintf("%s-%d", wipBranch, attempt+1)
		if attempt == 4 {
			return "", fmt.Errorf("Unable to create WIP branch for %s.", itemID)
		}
	}

	status, err := StatusDetails(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "status", err)
		return "", err
	}
	if status.HasTrackedChanges() || status.HasUntrackedChanges() {
		shortReason := CommitMessageShort(reason)
		commitCtx, cancel := withTimeout(ctx, gitCommitTimeout)
		defer cancel()
		if err := CommitAll(commitCtx, r.opts.RepoRoot, fmt.Sprintf("WIP %s: quarantine (%s)", itemID, shortReason)); err != nil {
			logGitError(r.redactor, r.opts.Logger, "commit", err)
			return "", err
		}
	}

	if err := CheckoutBranch(ctx, r.opts.RepoRoot, "main"); err != nil {
		return "", err
	}
	if err := ResetHard(ctx, r.opts.RepoRoot, headBefore); err != nil {
		return "", err
	}
	if r.opts.QuarantineClean {
		if err := Clean(ctx, r.opts.RepoRoot); err != nil {
			return "", err
		}
	}

	return wipBranch, nil
}

func (r *Runner) autoBlock(ctx context.Context, itemID string, reason string, wipBranch string, headBefore string) {
	unblockHint := fmt.Sprintf("Inspect %s and requeue once fixed.", wipBranch)
	reasons := []string{reason, fmt.Sprintf("Unblock: %s", unblockHint)}
	_, err := pin.BlockItem(r.pinFiles.QueuePath, itemID, reasons, pin.Metadata{
		WIPBranch:   wipBranch,
		KnownGood:   headBefore,
		UnblockHint: unblockHint,
	})
	if err != nil {
		r.logf("Error: Failed to move %s to Blocked via pin ops.", itemID)
		return
	}

	if r.opts.AutoCommit {
		shortReason := CommitMessageShort(reason)
		commitCtx, cancel := withTimeout(ctx, gitCommitTimeout)
		defer cancel()
		if err := CommitPaths(commitCtx, r.opts.RepoRoot, fmt.Sprintf("%s: auto-block (%s)", itemID, shortReason), r.pinFiles.QueuePath); err != nil {
			logGitError(r.redactor, r.opts.Logger, "commit", err)
			r.logf(">> [RALPH] Warning: auto-block commit failed; local pin edits remain.")
		}
		if r.opts.AutoPush {
			r.pushIfAhead(ctx)
		}
	}
}

func (r *Runner) pushIfAhead(ctx context.Context) {
	if r.pushCanceled {
		r.logf(">> [RALPH] Push skipped; cancellation already interrupted a push attempt.")
		return
	}
	ahead, err := AheadCount(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "ahead count", err)
		r.logf(">> [RALPH] Warning: unable to determine ahead count; skipping push.")
		r.pushFailed = true
		return
	}
	if ahead <= 0 {
		return
	}
	r.logf(">> [RALPH] Pushing %d commit(s) to upstream...", ahead)
	pushCtx, cancel := withTimeout(ctx, gitPushTimeout)
	defer cancel()
	if err := Push(pushCtx, r.opts.RepoRoot); err != nil {
		logGitError(r.redactor, r.opts.Logger, "push", err)
		if isCancellation(pushCtx, err) {
			r.logf(">> [RALPH] Push canceled; leaving commits locally.")
			r.pushCanceled = true
		}
		r.logf(">> [RALPH] Warning: git push failed; continuing with local commits.")
		r.pushFailed = true
		return
	}
	aheadAfter, err := AheadCount(ctx, r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "ahead count", err)
		r.logf(">> [RALPH] Warning: unable to verify upstream sync after push.")
		r.pushFailed = true
		return
	}
	if aheadAfter > 0 {
		r.logf(">> [RALPH] Warning: push did not bring HEAD in sync (ahead by %d).", aheadAfter)
		r.pushFailed = true
	}
}

func (r *Runner) logPushFailed() {
	if !r.pushFailed {
		return
	}
	ahead, err := AheadCount(context.Background(), r.opts.RepoRoot)
	if err != nil {
		logGitError(r.redactor, r.opts.Logger, "ahead count", err)
		r.logf(">> [RALPH] Push required; ahead count unavailable.")
		return
	}
	r.logf(">> [RALPH] Push required; local branch ahead by %d commit(s).", ahead)
}

func (r *Runner) logf(format string, args ...any) {
	if r.opts.Logger == nil {
		return
	}
	line := fmt.Sprintf(format, args...)
	if r.redactor != nil {
		line = r.redactor.Redact(line)
	}
	r.opts.Logger.WriteLine(line)
}

func failureMessage(message string, err error) string {
	if err == nil {
		return message
	}
	return fmt.Sprintf("%s: %v", message, err)
}

func isCancellation(ctx context.Context, err error) bool {
	if ctx != nil && ctx.Err() != nil {
		return true
	}
	if err == nil {
		return false
	}
	return errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded)
}

func withTimeout(ctx context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	if ctx == nil {
		ctx = context.Background()
	}
	if timeout <= 0 {
		return ctx, func() {}
	}
	return context.WithTimeout(ctx, timeout)
}

func (r *Runner) publishState(state State) {
	r.state = state
	if r.opts.StateSink != nil {
		r.opts.StateSink.Update(state)
	}
}

func (r *Runner) stateWithFailure(state State) State {
	state.LastFailureStage = r.lastFailureStage
	state.LastFailureMessage = r.lastFailureMessage
	return state
}

func (r *Runner) recordFailure(stage string, message string) {
	r.lastFailureStage = stage
	r.lastFailureMessage = message
	r.publishState(r.stateWithFailure(r.state))
}

func contextBuilderPolicyBlock(effort string, mandatory bool, forced bool) string {
	builder := strings.Builder{}
	builder.WriteString("# CODEX CONTEXT BUILDER POLICY\n")
	builder.WriteString(fmt.Sprintf("Codex model_reasoning_effort: %s\n", effort))
	if forced {
		builder.WriteString("Override: Force context_builder is ENABLED.\n")
	}
	if mandatory {
		if forced {
			builder.WriteString("MANDATORY: Force context_builder override is enabled; you MUST use the repo_prompt context_builder to gather context and generate a plan BEFORE making code changes.\n")
		} else {
			builder.WriteString("MANDATORY: Because reasoning effort is low/off, you MUST use the repo_prompt context_builder to gather context and generate a plan BEFORE making code changes.\n")
		}
		builder.WriteString("Execute the plan it generates.\n")
	} else {
		builder.WriteString("OPTIONAL: You MAY use the repo_prompt context_builder to gather context and generate a plan. It is recommended for complex items or difficult root-cause triage.\n")
	}
	return builder.String()
}

func pinPathPrefix(repoRoot string, pinDir string) string {
	rel, err := filepath.Rel(repoRoot, pinDir)
	if err != nil || strings.HasPrefix(rel, "..") {
		rel = pinDir
	}
	rel = filepath.ToSlash(strings.TrimPrefix(rel, "./"))
	if !strings.HasSuffix(rel, "/") {
		rel += "/"
	}
	return rel
}
