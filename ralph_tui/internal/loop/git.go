// Package loop provides git helpers for the Ralph loop.
// Entrypoint: CurrentBranch, HeadSHA, StatusPorcelain.
package loop

import (
	"bytes"
	"fmt"
	"os/exec"
	"strings"
)

func CurrentBranch(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "rev-parse", "--abbrev-ref", "HEAD")
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("Unable to detect current git branch.")
	}
	branch := strings.TrimSpace(string(out))
	if branch == "" {
		return "", fmt.Errorf("Unable to detect current git branch.")
	}
	return branch, nil
}

func HeadSHA(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "rev-parse", "HEAD")
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func StatusPorcelain(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "status", "--porcelain")
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func DiffNameOnly(repoRoot string) ([]string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "diff", "--name-only")
	out, err := cmd.Output()
	if err != nil {
		return nil, err
	}
	trimmed := strings.TrimSpace(string(out))
	if trimmed == "" {
		return []string{}, nil
	}
	return strings.Split(trimmed, "\n"), nil
}

func DiffStat(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "diff", "--stat")
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func Diff(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "diff")
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return string(out), nil
}

func StatusSummary(repoRoot string) (string, error) {
	cmd := exec.Command("git", "-C", repoRoot, "status", "-sb")
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func CommitAll(repoRoot string, message string) error {
	cmd := exec.Command("git", "-C", repoRoot, "add", "-A")
	if err := cmd.Run(); err != nil {
		return err
	}
	cmd = exec.Command("git", "-C", repoRoot, "commit", "-m", message)
	cmd.Stdout = nil
	cmd.Stderr = nil
	return cmd.Run()
}

func CommitPaths(repoRoot string, message string, paths ...string) error {
	args := append([]string{"-C", repoRoot, "add"}, paths...)
	cmd := exec.Command("git", args...)
	if err := cmd.Run(); err != nil {
		return err
	}
	cmd = exec.Command("git", "-C", repoRoot, "commit", "-m", message)
	cmd.Stdout = nil
	cmd.Stderr = nil
	return cmd.Run()
}

func CheckoutBranch(repoRoot string, branch string) error {
	cmd := exec.Command("git", "-C", repoRoot, "checkout", branch)
	return cmd.Run()
}

func CheckoutNewBranch(repoRoot string, branch string) error {
	cmd := exec.Command("git", "-C", repoRoot, "checkout", "-b", branch)
	return cmd.Run()
}

func ResetHard(repoRoot string, sha string) error {
	cmd := exec.Command("git", "-C", repoRoot, "reset", "--hard", sha)
	return cmd.Run()
}

func Clean(repoRoot string) error {
	cmd := exec.Command("git", "-C", repoRoot, "clean", "-fd")
	return cmd.Run()
}

func AheadCount(repoRoot string) (int, error) {
	cmd := exec.Command("git", "-C", repoRoot, "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}")
	if err := cmd.Run(); err != nil {
		return 0, nil
	}
	cmd = exec.Command("git", "-C", repoRoot, "rev-list", "--count", "@{u}..HEAD")
	out, err := cmd.Output()
	if err != nil {
		return 0, nil
	}
	trimmed := strings.TrimSpace(string(out))
	if trimmed == "" {
		return 0, nil
	}
	count := 0
	fmt.Sscanf(trimmed, "%d", &count)
	return count, nil
}

func Push(repoRoot string) error {
	cmd := exec.Command("git", "-C", repoRoot, "push")
	cmd.Stdout = nil
	cmd.Stderr = nil
	return cmd.Run()
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
