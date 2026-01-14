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
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
)

// Options controls loop execution.
type Options struct {
	RepoRoot          string
	PinDir            string
	PromptPath        string
	SupervisorPrompt  string
	Runner            string
	RunnerArgs        []string
	SleepSeconds      int
	MaxIterations     int
	MaxStalled        int
	MaxRepairAttempts int
	OnlyTags          []string
	Once              bool
	RequireMain       bool
	AutoCommit        bool
	AutoPush          bool
	RedactionMode     redaction.Mode
	Logger            Logger
}

// Runner executes the loop.
type Runner struct {
	opts                    Options
	redactor                *Redactor
	pinFiles                pin.Files
	pushFailed              bool
	lastValidateOutput      string
	lastCIOutput            string
	lastFailureStage        string
	lastFailureMessage      string
	currentRunArgs          []string
	currentItemBlock        string
	effectiveEffort         string
	contextBuilderMandatory bool
}

// NewRunner constructs a loop runner.
func NewRunner(opts Options) (*Runner, error) {
	if opts.RepoRoot == "" {
		return nil, fmt.Errorf("repo root required")
	}
	if opts.PinDir == "" {
		return nil, fmt.Errorf("pin dir required")
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
	if err := r.verifyBranch(); err != nil {
		return err
	}

	lock, err := specs.AcquireLock(r.opts.RepoRoot)
	if err != nil {
		return err
	}
	defer lock.Release()

	iterations := 0
	stalled := 0

	for {
		select {
		case <-ctx.Done():
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
			r.logPushFailed()
			return nil
		}

		itemID := firstItem.ID
		if itemID == "" {
			return fmt.Errorf("Queue item is missing an ID prefix (expected something like ABC-0123).")
		}

		effort := "medium"
		if strings.Contains(firstItem.Header, "[P1]") {
			effort = "high"
		}
		r.currentRunArgs = append([]string{}, r.opts.RunnerArgs...)
		if r.opts.Runner == "codex" {
			if !containsEffort(r.currentRunArgs) {
				r.currentRunArgs = append([]string{"-c", fmt.Sprintf("model_reasoning_effort=\"%s\"", effort)}, r.currentRunArgs...)
			}
			r.effectiveEffort = detectEffort(r.currentRunArgs, effort)
			r.contextBuilderMandatory = r.effectiveEffort == "low" || r.effectiveEffort == "off"
		} else {
			r.effectiveEffort = ""
			r.contextBuilderMandatory = false
		}

		headBefore, err := HeadSHA(r.opts.RepoRoot)
		if err != nil {
			return err
		}

		block, err := CurrentItemBlock(r.pinFiles.QueuePath, itemID)
		if err != nil {
			return err
		}
		r.currentItemBlock = block

		if err := r.reconcileCheckedQueueItems(); err != nil {
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "pin-ops", "Failed to move checked queue items.")
			continue
		}

		headBefore, err = HeadSHA(r.opts.RepoRoot)
		if err != nil {
			return err
		}

		dirty, err := StatusPorcelain(r.opts.RepoRoot)
		if err != nil {
			return err
		}
		if dirty != "" {
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "preflight", fmt.Sprintf("Working tree is dirty before iteration %d.", iterations))
			continue
		}

		if err := r.runValidatePin(); err != nil {
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "pin-validate", fmt.Sprintf("validate_pin failed before iteration %d.", iterations))
			continue
		}

		iterations++
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
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "runner", fmt.Sprintf("%s failed on iteration %d.", r.opts.Runner, iterations))
			continue
		}
		cleanup()

		if err := r.finalizeIteration(itemID, firstItem.Header, headBefore); err != nil {
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, r.lastFailureStage, r.lastFailureMessage)
			continue
		}

		dirty, err = StatusPorcelain(r.opts.RepoRoot)
		if err != nil {
			return err
		}
		if dirty != "" {
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "post-commit", fmt.Sprintf("Working tree is dirty after iteration %d.", iterations))
			continue
		}

		r.cleanupIterationArtifacts()

		firstAfter, err := FirstUncheckedItem(r.pinFiles.QueuePath, r.opts.OnlyTags)
		if err != nil {
			return err
		}
		headAfter, err := HeadSHA(r.opts.RepoRoot)
		if err != nil {
			return err
		}
		if headBefore == headAfter && firstAfter != nil && firstAfter.Header == firstItem.Header {
			stalled++
		} else {
			stalled = 0
		}

		if r.opts.MaxStalled > 0 && stalled >= r.opts.MaxStalled {
			r.handleIterationFailure(ctx, itemID, firstItem.Header, headBefore, "stall", fmt.Sprintf("Stalled for %d iterations (head and first queue item unchanged).", stalled))
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

func (r *Runner) verifyBranch() error {
	if !r.opts.RequireMain {
		return nil
	}
	branch, err := CurrentBranch(r.opts.RepoRoot)
	if err != nil {
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
		builder.WriteString(contextBuilderPolicyBlock(r.effectiveEffort, r.contextBuilderMandatory))
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

func (r *Runner) finalizeIteration(itemID string, itemLine string, headBefore string) error {
	r.lastFailureStage = ""
	r.lastFailureMessage = ""

	headNow, err := HeadSHA(r.opts.RepoRoot)
	if err != nil {
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

	dirty, err := StatusPorcelain(r.opts.RepoRoot)
	if err != nil {
		return err
	}

	if !completed {
		if dirty != "" {
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

	if dirty == "" {
		r.lastFailureStage = "complete"
		r.lastFailureMessage = "Queue head moved but no changes detected."
		return errors.New(r.lastFailureMessage)
	}

	onlySpecs := true
	pinPrefix := pinPathPrefix(r.opts.RepoRoot, r.opts.PinDir)
	changed, err := DiffNameOnly(r.opts.RepoRoot)
	if err != nil {
		return err
	}
	for _, path := range changed {
		if !strings.HasPrefix(path, pinPrefix) {
			onlySpecs = false
			break
		}
	}

	if !onlySpecs {
		if err := r.runMakeCI(); err != nil {
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
		if err := CommitAll(r.opts.RepoRoot, fmt.Sprintf("%s: %s", itemID, title)); err != nil {
			return err
		}
		if r.opts.AutoPush {
			r.pushIfAhead()
		}
	}

	if err := r.runValidatePin(); err != nil {
		r.lastFailureStage = "pin-validate"
		r.lastFailureMessage = "validate_pin failed after commit."
		return err
	}

	return nil
}

func (r *Runner) reconcileCheckedQueueItems() error {
	movedIDs, err := pin.MoveCheckedToDone(r.pinFiles.QueuePath, r.pinFiles.DonePath, true)
	if err != nil {
		return err
	}
	summary := pin.SummarizeIDs(movedIDs)
	if summary != "" {
		dirty, err := StatusPorcelain(r.opts.RepoRoot)
		if err != nil {
			return err
		}
		if dirty != "" && r.opts.AutoCommit {
			if err := CommitPaths(r.opts.RepoRoot, fmt.Sprintf("chore: move completed queue items (%s)", summary), r.pinFiles.QueuePath, r.pinFiles.DonePath); err != nil {
				return err
			}
			if r.opts.AutoPush {
				r.pushIfAhead()
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

func (r *Runner) runMakeCI() error {
	file, err := os.CreateTemp("", "ralph_make_ci_*.log")
	if err != nil {
		return err
	}
	r.lastCIOutput = file.Name()
	_ = file.Close()

	cmd := exec.Command("make", "-C", r.opts.RepoRoot, "ci")
	if err := RunCommandWithFile(cmd, r.redactor, r.opts.Logger, r.lastCIOutput); err != nil {
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

func (r *Runner) handleIterationFailure(ctx context.Context, itemID string, itemLine string, headBefore string, stage string, message string) {
	r.logf(">> [RALPH] Iteration failure (%s): %s", stage, message)

	attempt := 1
	for attempt <= r.opts.MaxRepairAttempts {
		r.logf(">> [RALPH] Supervisor attempt %d/%d...", attempt, r.opts.MaxRepairAttempts)
		r.runSupervisor(ctx, stage, message)
		if r.lastFailureStage == "" {
			if err := r.finalizeIteration(itemID, itemLine, headBefore); err == nil {
				r.cleanupIterationArtifacts()
				return
			}
			stage = r.lastFailureStage
			message = r.lastFailureMessage
		} else {
			stage = "supervisor"
			message = "Supervisor runner failed."
		}
		attempt++
	}

	wipBranch, err := r.quarantine(itemID, headBefore, message)
	if err != nil {
		r.logf("Error: %s", err.Error())
		return
	}
	r.autoBlock(itemID, message, wipBranch, headBefore)
	r.cleanupIterationArtifacts()
}

func (r *Runner) runSupervisor(ctx context.Context, stage string, message string) {
	contextFile, cleanup, err := r.buildSupervisorContext(stage, message)
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

func (r *Runner) buildSupervisorContext(stage string, message string) (string, func(), error) {
	content, err := r.loadSupervisorPrompt()
	if err != nil {
		return "", func() {}, err
	}

	builder := strings.Builder{}
	builder.WriteString(content)
	builder.WriteString("\n\n")
	if r.opts.Runner == "codex" && r.effectiveEffort != "" {
		builder.WriteString(contextBuilderPolicyBlock(r.effectiveEffort, r.contextBuilderMandatory))
		builder.WriteString("\n\n")
	}
	builder.WriteString("# FAILURE CONTEXT\n")
	builder.WriteString(fmt.Sprintf("Stage: %s\n", stage))
	builder.WriteString(fmt.Sprintf("Message: %s\n\n", message))
	builder.WriteString("# CURRENT QUEUE ITEM\n")
	builder.WriteString(r.currentItemBlock)
	builder.WriteString("\n\n")

	status, _ := StatusSummary(r.opts.RepoRoot)
	builder.WriteString("# GIT STATUS\n")
	builder.WriteString(status)
	builder.WriteString("\n\n")

	stat, _ := DiffStat(r.opts.RepoRoot)
	builder.WriteString("# GIT DIFF --STAT\n")
	builder.WriteString(stat)
	builder.WriteString("\n\n")

	diff, _ := Diff(r.opts.RepoRoot)
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

func (r *Runner) quarantine(itemID string, headBefore string, reason string) (string, error) {
	wipBranch := CreateWipBranchName(itemID, time.Now().Format("20060102_150405"))
	candidate := wipBranch
	for attempt := 0; attempt < 5; attempt++ {
		if err := CheckoutNewBranch(r.opts.RepoRoot, candidate); err == nil {
			wipBranch = candidate
			break
		}
		candidate = fmt.Sprintf("%s-%d", wipBranch, attempt+1)
		if attempt == 4 {
			return "", fmt.Errorf("Unable to create WIP branch for %s.", itemID)
		}
	}

	dirty, _ := StatusPorcelain(r.opts.RepoRoot)
	if dirty != "" {
		shortReason := CommitMessageShort(reason)
		_ = CommitAll(r.opts.RepoRoot, fmt.Sprintf("WIP %s: quarantine (%s)", itemID, shortReason))
	}

	if err := CheckoutBranch(r.opts.RepoRoot, "main"); err != nil {
		return "", err
	}
	if err := ResetHard(r.opts.RepoRoot, headBefore); err != nil {
		return "", err
	}
	if err := Clean(r.opts.RepoRoot); err != nil {
		return "", err
	}

	return wipBranch, nil
}

func (r *Runner) autoBlock(itemID string, reason string, wipBranch string, headBefore string) {
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
		_ = CommitPaths(r.opts.RepoRoot, fmt.Sprintf("%s: auto-block (%s)", itemID, shortReason), r.pinFiles.QueuePath)
		if r.opts.AutoPush {
			r.pushIfAhead()
		}
	}
}

func (r *Runner) pushIfAhead() {
	ahead, _ := AheadCount(r.opts.RepoRoot)
	if ahead <= 0 {
		return
	}
	r.logf(">> [RALPH] Pushing %d commit(s) to upstream...", ahead)
	if err := Push(r.opts.RepoRoot); err != nil {
		r.logf(">> [RALPH] Warning: git push failed; continuing with local commits.")
		r.pushFailed = true
		return
	}
	aheadAfter, _ := AheadCount(r.opts.RepoRoot)
	if aheadAfter > 0 {
		r.logf(">> [RALPH] Warning: push did not bring HEAD in sync (ahead by %d).", aheadAfter)
		r.pushFailed = true
	}
}

func (r *Runner) logPushFailed() {
	if !r.pushFailed {
		return
	}
	ahead, _ := AheadCount(r.opts.RepoRoot)
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

func contextBuilderPolicyBlock(effort string, mandatory bool) string {
	builder := strings.Builder{}
	builder.WriteString("# CODEX CONTEXT BUILDER POLICY\n")
	builder.WriteString(fmt.Sprintf("Codex model_reasoning_effort: %s\n", effort))
	if mandatory {
		builder.WriteString("MANDATORY: Because reasoning effort is low/off, you MUST use the repo_prompt context_builder to gather context and generate a plan BEFORE making code changes.\n")
		builder.WriteString("Execute the plan it generates.\n")
	} else {
		builder.WriteString("OPTIONAL: You MAY use the repo_prompt context_builder to gather context and generate a plan. It is recommended for complex items or difficult root-cause triage.\n")
	}
	return builder.String()
}

func containsEffort(args []string) bool {
	for _, token := range args {
		if strings.Contains(token, "model_reasoning_effort") {
			return true
		}
	}
	return false
}

func detectEffort(args []string, defaultEffort string) string {
	detected := defaultEffort
	for idx := 0; idx < len(args); idx++ {
		token := args[idx]
		if token == "-c" && idx+1 < len(args) {
			if value, ok := extractEffort(args[idx+1]); ok {
				detected = value
			}
			idx++
			continue
		}
		if strings.Contains(token, "model_reasoning_effort") {
			if value, ok := extractEffort(token); ok {
				detected = value
			}
		}
	}
	return detected
}

func extractEffort(config string) (string, bool) {
	idx := strings.Index(config, "model_reasoning_effort")
	if idx == -1 {
		return "", false
	}
	parts := strings.SplitN(config[idx:], "=", 2)
	if len(parts) != 2 {
		return "", false
	}
	value := strings.Trim(parts[1], "\"'")
	value = strings.TrimSpace(value)
	if value == "" {
		return "", false
	}
	return strings.ToLower(value), true
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
