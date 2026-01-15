// Package loop provides git helpers for the Ralph loop.
// Entrypoint: CurrentBranch, HeadSHA, StatusPorcelain.
package loop

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strings"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/procgroup"
)

const (
	gitOutputTailLines = 20
	defaultGitTimeout  = 2 * time.Minute
)

// GitCommandError wraps a git failure with trimmed stdout/stderr details.
type GitCommandError struct {
	Command string
	Err     error
	Stdout  string
	Stderr  string
}

func (e *GitCommandError) Error() string {
	if e == nil {
		return ""
	}
	if e.Err == nil {
		return fmt.Sprintf("git command failed (%s)", e.Command)
	}
	return fmt.Sprintf("git command failed (%s): %v", e.Command, e.Err)
}

func (e *GitCommandError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

func (e *GitCommandError) DetailLines() []string {
	if e == nil {
		return nil
	}
	lines := []string{e.Error()}
	if strings.TrimSpace(e.Stderr) != "" {
		lines = append(lines, "stderr (tail):")
		lines = append(lines, strings.Split(e.Stderr, "\n")...)
	}
	if strings.TrimSpace(e.Stdout) != "" {
		lines = append(lines, "stdout (tail):")
		lines = append(lines, strings.Split(e.Stdout, "\n")...)
	}
	return lines
}

func newGitCommandError(err error, repoRoot string, args []string, stdout string, stderr string) error {
	if err == nil {
		return nil
	}
	command := fmt.Sprintf("git -C %s %s", repoRoot, strings.Join(args, " "))
	return &GitCommandError{
		Command: command,
		Err:     err,
		Stdout:  StringTail(stdout, gitOutputTailLines),
		Stderr:  StringTail(stderr, gitOutputTailLines),
	}
}

func gitOutput(ctx context.Context, repoRoot string, args ...string) (string, error) {
	stdout, stderr, err := runGitCommand(ctx, repoRoot, args...)
	if err != nil {
		return "", newGitCommandError(err, repoRoot, args, stdout, stderr)
	}
	return stdout, nil
}

func gitRun(ctx context.Context, repoRoot string, args ...string) error {
	stdout, stderr, err := runGitCommand(ctx, repoRoot, args...)
	if err != nil {
		return newGitCommandError(err, repoRoot, args, stdout, stderr)
	}
	return nil
}

func runGitCommand(ctx context.Context, repoRoot string, args ...string) (string, string, error) {
	allArgs := append([]string{"-C", repoRoot}, args...)
	if ctx == nil {
		ctx = context.Background()
	}
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, defaultGitTimeout)
		defer cancel()
	}
	cmd := exec.CommandContext(ctx, "git", allArgs...)
	procgroup.Configure(cmd)
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}

func logGitError(redactor *Redactor, logger Logger, context string, err error) {
	if logger == nil || err == nil {
		return
	}
	var gitErr *GitCommandError
	if !errors.As(err, &gitErr) {
		return
	}
	lines := gitErr.DetailLines()
	if len(lines) == 0 {
		return
	}
	if context != "" {
		lines[0] = fmt.Sprintf("%s (%s)", lines[0], context)
	}
	for _, line := range lines {
		if redactor != nil {
			line = redactor.Redact(line)
		}
		logger.WriteLine(">> [RALPH] " + line)
	}
}

func CurrentBranch(ctx context.Context, repoRoot string) (string, error) {
	out, err := gitOutput(ctx, repoRoot, "rev-parse", "--abbrev-ref", "HEAD")
	if err != nil {
		return "", err
	}
	branch := strings.TrimSpace(out)
	if branch == "" {
		return "", fmt.Errorf("git rev-parse returned an empty branch name")
	}
	return branch, nil
}

func HeadSHA(ctx context.Context, repoRoot string) (string, error) {
	out, err := gitOutput(ctx, repoRoot, "rev-parse", "HEAD")
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(out), nil
}

func StatusPorcelain(ctx context.Context, repoRoot string) (string, error) {
	out, err := gitOutput(ctx, repoRoot, "status", "--porcelain")
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(out), nil
}

func DiffNameOnly(ctx context.Context, repoRoot string) ([]string, error) {
	out, err := gitOutput(ctx, repoRoot, "diff", "--name-only")
	if err != nil {
		return nil, err
	}
	trimmed := strings.TrimSpace(out)
	if trimmed == "" {
		return []string{}, nil
	}
	return strings.Split(trimmed, "\n"), nil
}

func DiffNameOnlyRange(ctx context.Context, repoRoot string, from string, to string) ([]string, error) {
	rangeSpec := fmt.Sprintf("%s..%s", from, to)
	out, err := gitOutput(ctx, repoRoot, "diff", "--name-only", rangeSpec)
	if err != nil {
		return nil, err
	}
	trimmed := strings.TrimSpace(out)
	if trimmed == "" {
		return []string{}, nil
	}
	return strings.Split(trimmed, "\n"), nil
}

func DiffStat(ctx context.Context, repoRoot string) (string, error) {
	out, err := gitOutput(ctx, repoRoot, "diff", "--stat")
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(out), nil
}

func Diff(ctx context.Context, repoRoot string) (string, error) {
	out, err := gitOutput(ctx, repoRoot, "diff")
	if err != nil {
		return "", err
	}
	return out, nil
}

func StatusSummary(ctx context.Context, repoRoot string) (string, error) {
	out, err := gitOutput(ctx, repoRoot, "status", "-sb")
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(out), nil
}

func CommitAll(ctx context.Context, repoRoot string, message string) error {
	if err := gitRun(ctx, repoRoot, "add", "-A"); err != nil {
		return err
	}
	return gitRun(ctx, repoRoot, "commit", "-m", message)
}

func CommitPaths(ctx context.Context, repoRoot string, message string, paths ...string) error {
	args := append([]string{"add"}, paths...)
	if err := gitRun(ctx, repoRoot, args...); err != nil {
		return err
	}
	return gitRun(ctx, repoRoot, "commit", "-m", message)
}

func CheckoutBranch(ctx context.Context, repoRoot string, branch string) error {
	return gitRun(ctx, repoRoot, "checkout", branch)
}

func CheckoutNewBranch(ctx context.Context, repoRoot string, branch string) error {
	return gitRun(ctx, repoRoot, "checkout", "-b", branch)
}

func BranchExists(ctx context.Context, repoRoot string, branch string) (bool, error) {
	err := gitRun(ctx, repoRoot, "rev-parse", "--verify", "--quiet", branch+"^{commit}")
	if err != nil {
		if _, ok := err.(*exec.ExitError); ok {
			return false, nil
		}
		return false, err
	}
	return true, nil
}

func ResetHard(ctx context.Context, repoRoot string, sha string) error {
	return gitRun(ctx, repoRoot, "reset", "--hard", sha)
}

func WorktreeAddDetach(ctx context.Context, repoRoot string, path string, ref string) error {
	return gitRun(ctx, repoRoot, "worktree", "add", "--detach", path, ref)
}

func WorktreeRemove(ctx context.Context, repoRoot string, path string) error {
	return gitRun(ctx, repoRoot, "worktree", "remove", "--force", path)
}

func Clean(ctx context.Context, repoRoot string) error {
	return gitRun(ctx, repoRoot, "clean", "-fd")
}

func AheadCount(ctx context.Context, repoRoot string) (int, error) {
	if err := gitRun(ctx, repoRoot, "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"); err != nil {
		return 0, err
	}
	out, err := gitOutput(ctx, repoRoot, "rev-list", "--count", "@{u}..HEAD")
	if err != nil {
		return 0, err
	}
	trimmed := strings.TrimSpace(out)
	if trimmed == "" {
		return 0, fmt.Errorf("git rev-list returned empty output")
	}
	count := 0
	fmt.Sscanf(trimmed, "%d", &count)
	return count, nil
}

func Push(ctx context.Context, repoRoot string) error {
	return gitRun(ctx, repoRoot, "push")
}

func CommitMessageShort(reason string) string {
	compact := strings.Join(strings.Fields(reason), " ")
	if len(compact) > 60 {
		return compact[:57] + "..."
	}
	return compact
}

func CreateWipBranchName(itemID string, ts string) string {
	return fmt.Sprintf("ralph/wip/%s/%s", itemID, ts)
}

func StringTail(input string, maxLines int) string {
	lines := strings.Split(strings.TrimSuffix(input, "\n"), "\n")
	if len(lines) <= maxLines {
		return strings.Join(lines, "\n")
	}
	return strings.Join(lines[len(lines)-maxLines:], "\n")
}

func bytesToLines(data []byte) []string {
	trimmed := strings.TrimSuffix(string(data), "\n")
	if trimmed == "" {
		return []string{}
	}
	return strings.Split(trimmed, "\n")
}

func joinLines(lines []string) string {
	return strings.Join(lines, "\n")
}

func bufferLines(buf *bytes.Buffer) []string {
	return bytesToLines(buf.Bytes())
}
