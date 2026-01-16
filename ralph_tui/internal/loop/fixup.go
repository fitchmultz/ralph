// Package loop provides fixup helpers for blocked queue items.
// Entrypoint: FixupBlockedItems.
package loop

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
)

// FixupOptions controls the blocked-item fixup workflow.
type FixupOptions struct {
	RepoRoot            string
	PinDir              string
	ProjectType         project.Type
	MaxAttempts         int
	MaxItems            int
	RequireMain         bool
	AutoCommit          bool
	AutoPush            bool
	RedactionMode       redaction.Mode
	LogMaxBufferedBytes int
	Logger              Logger
	Now                 func() time.Time
}

// FixupResult summarizes fixup outcomes.
type FixupResult struct {
	ScannedBlocked int
	Eligible       int
	RequeuedIDs    []string
	SkippedMax     []string
	FailedIDs      []string
	FailedReasons  []FixupFailure
}

// FixupFailure captures a failed fixup attempt with context.
type FixupFailure struct {
	ID     string
	Reason string
}

// FixupBlockedItems attempts to validate and requeue blocked items with WIP metadata.
func FixupBlockedItems(ctx context.Context, opts FixupOptions) (FixupResult, error) {
	result := FixupResult{}
	if opts.RepoRoot == "" {
		return result, fmt.Errorf("repo root is required")
	}
	if opts.PinDir == "" {
		return result, fmt.Errorf("pin dir is required")
	}
	resolvedProjectType, err := project.ResolveType(opts.ProjectType)
	if err != nil {
		return result, fmt.Errorf("project_type must be code or docs")
	}
	opts.ProjectType = resolvedProjectType

	redactor := NewRedactor(os.Environ(), opts.RedactionMode)
	logFixup(redactor, opts.Logger, ">> [RALPH] Fixup acquiring lock.")
	lock, err := specs.AcquireLock(opts.RepoRoot)
	if err != nil {
		logFixup(redactor, opts.Logger, ">> [RALPH] Fixup failed to acquire lock: %s", err.Error())
		return result, err
	}
	defer lock.Release()
	logFixup(redactor, opts.Logger, ">> [RALPH] Fixup lock acquired.")
	now := opts.Now
	if now == nil {
		now = time.Now
	}

	if opts.RequireMain {
		branch, err := CurrentBranch(ctx, opts.RepoRoot)
		if err != nil {
			logGitError(redactor, opts.Logger, "current branch", err)
			return result, err
		}
		if branch != "main" {
			return result, fmt.Errorf("fixup requires main branch (current: %s)", branch)
		}
	}

	dirty, err := StatusPorcelain(ctx, opts.RepoRoot)
	if err != nil {
		logGitError(redactor, opts.Logger, "status", err)
		return result, err
	}
	if dirty != "" {
		return result, fmt.Errorf("fixup requires a clean working tree")
	}

	queuePath := filepath.Join(opts.PinDir, "implementation_queue.md")
	blockedItems, err := pin.ReadBlockedItems(queuePath)
	if err != nil {
		return result, err
	}
	result.ScannedBlocked = len(blockedItems)
	logFixup(redactor, opts.Logger, ">> [RALPH] Fixup scanning %d blocked items.", result.ScannedBlocked)

	attempted := 0
	for _, item := range blockedItems {
		if opts.MaxItems > 0 && attempted >= opts.MaxItems {
			break
		}
		if item.Metadata.WIPBranch == "" || item.Metadata.KnownGood == "" {
			continue
		}
		result.Eligible++
		if opts.MaxAttempts > 0 && item.FixupAttempts >= opts.MaxAttempts {
			result.SkippedMax = append(result.SkippedMax, item.ID)
			continue
		}

		attempted++
		logFixup(redactor, opts.Logger, ">> [RALPH] Fixup %s using %s", item.ID, item.Metadata.WIPBranch)
		err := validateWipBranchInWorktree(ctx, opts, redactor, item.Metadata.WIPBranch, item.Metadata.KnownGood)
		if err == nil {
			updated, err := pin.RequeueBlockedItem(queuePath, item.ID, pin.RequeueOptions{InsertAtTop: true})
			if err != nil {
				return result, err
			}
			if !updated {
				return result, fmt.Errorf("blocked item %s not found during requeue", item.ID)
			}
			if err := pin.ValidatePin(pin.ResolveFiles(opts.PinDir), opts.ProjectType); err != nil {
				return result, err
			}
			if err := commitPinChanges(opts, redactor, fmt.Sprintf("%s: fixup requeue", item.ID)); err != nil {
				return result, err
			}
			result.RequeuedIDs = append(result.RequeuedIDs, item.ID)
			continue
		}

		logFixup(redactor, opts.Logger, ">> [RALPH] Fixup %s failed: %s", item.ID, err.Error())
		result.FailedReasons = append(result.FailedReasons, FixupFailure{
			ID:     item.ID,
			Reason: err.Error(),
		})
		reason := fmt.Sprintf("%s %s", now().Format(time.RFC3339), CommitMessageShort(err.Error()))
		updated, attempts, err := pin.RecordFixupAttempt(queuePath, item.ID, reason)
		if err != nil {
			return result, err
		}
		if !updated {
			return result, fmt.Errorf("blocked item %s not found during attempt update", item.ID)
		}
		if err := pin.ValidatePin(pin.ResolveFiles(opts.PinDir), opts.ProjectType); err != nil {
			return result, err
		}
		if err := commitPinChanges(opts, redactor, fmt.Sprintf("%s: fixup attempt %d", item.ID, attempts)); err != nil {
			return result, err
		}
		result.FailedIDs = append(result.FailedIDs, item.ID)
	}

	return result, nil
}

func validateWipBranchInWorktree(ctx context.Context, opts FixupOptions, redactor *Redactor, wipBranch string, knownGood string) error {
	exists, err := BranchExists(ctx, opts.RepoRoot, wipBranch)
	if err != nil {
		return err
	}
	if !exists {
		return fmt.Errorf("wip branch %s not found", wipBranch)
	}

	worktreePath, err := os.MkdirTemp("", "ralph_fixup_worktree_")
	if err != nil {
		return err
	}
	defer func() {
		_ = WorktreeRemove(context.Background(), opts.RepoRoot, worktreePath)
		_ = os.RemoveAll(worktreePath)
	}()

	if err := WorktreeAddDetach(ctx, opts.RepoRoot, worktreePath, wipBranch); err != nil {
		return err
	}

	worktreePinDir := pinDirForWorktree(opts.RepoRoot, opts.PinDir, worktreePath)
	if err := pin.ValidatePin(pin.ResolveFiles(worktreePinDir), opts.ProjectType); err != nil {
		return err
	}

	pinPrefix := pinPathPrefix(opts.RepoRoot, opts.PinDir)
	changed, err := DiffNameOnlyRange(ctx, opts.RepoRoot, knownGood, wipBranch)
	if err != nil {
		return err
	}
	if !pathsOnlyUnderPrefix(changed, pinPrefix) {
		return runMakeCIInWorktree(ctx, opts, redactor, worktreePath)
	}
	logFixup(redactor, opts.Logger, ">> [RALPH] Fixup skipping make ci (pin-only changes).")

	return nil
}

func runMakeCIInWorktree(ctx context.Context, opts FixupOptions, redactor *Redactor, worktreePath string) error {
	logFixup(redactor, opts.Logger, ">> [RALPH] Fixup running make ci in %s", worktreePath)
	cmd := exec.CommandContext(ctx, "make", "-C", worktreePath, "ci")
	if err := RunCommand(ctx, cmd, NewRedactor(os.Environ(), opts.RedactionMode), opts.Logger, opts.LogMaxBufferedBytes); err != nil {
		return err
	}
	logFixup(redactor, opts.Logger, ">> [RALPH] Fixup make ci succeeded.")
	return nil
}

func pinDirForWorktree(repoRoot string, pinDir string, worktreePath string) string {
	rel, err := filepath.Rel(repoRoot, pinDir)
	if err != nil || strings.HasPrefix(rel, "..") {
		return filepath.Join(worktreePath, filepath.Base(pinDir))
	}
	rel = strings.TrimPrefix(rel, string(os.PathSeparator))
	return filepath.Join(worktreePath, rel)
}

func commitPinChanges(opts FixupOptions, redactor *Redactor, message string) error {
	if !opts.AutoCommit {
		return nil
	}
	status, err := StatusPorcelain(context.Background(), opts.RepoRoot)
	if err != nil {
		logGitError(redactor, opts.Logger, "status", err)
		return err
	}
	if status == "" {
		return nil
	}

	queuePath := filepath.Join(opts.PinDir, "implementation_queue.md")
	if err := CommitPaths(context.Background(), opts.RepoRoot, message, queuePath); err != nil {
		logGitError(redactor, opts.Logger, "commit", err)
		return err
	}
	if !opts.AutoPush {
		return nil
	}
	ahead, err := AheadCount(context.Background(), opts.RepoRoot)
	if err != nil {
		logGitError(redactor, opts.Logger, "ahead count", err)
		return err
	}
	if ahead <= 0 {
		return nil
	}
	if err := Push(context.Background(), opts.RepoRoot); err != nil {
		logGitError(redactor, opts.Logger, "push", err)
		return fmt.Errorf("git push failed: %w", err)
	}
	return nil
}

func logFixup(redactor *Redactor, logger Logger, format string, args ...any) {
	if logger == nil {
		return
	}
	line := fmt.Sprintf(format, args...)
	if redactor != nil {
		line = redactor.Redact(line)
	}
	logger.WriteLine(line)
}
