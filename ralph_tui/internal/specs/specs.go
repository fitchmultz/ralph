// Package specs builds Ralph specs prompts and invokes runners.
// Entrypoint: Build, FillPrompt.
package specs

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

const (
	RunnerCodex    Runner = "codex"
	RunnerOpencode Runner = "opencode"
)

const (
	interactivePlaceholder = "{{INTERACTIVE_INSTRUCTIONS}}"
	innovatePlaceholder    = "{{INNOVATE_INSTRUCTIONS}}"
)

const interactiveInstructions = "INTERACTIVE MODE ENABLED. Before adding any new queue items:\n" +
	"1) List the candidate items you intend to add (bulleted).\n" +
	"2) Ask the user for directives/approval or edits.\n" +
	"3) Wait for the user's response, then incorporate it.\n" +
	"If no new items are proposed, ask the user if they want any new directions.\n"

const innovateInstructions = "AUTOFILL/SCOUT MODE ENABLED (AGGRESSIVE).\n" +
	"\n" +
	"This repo intentionally avoids TODO/TBD placeholders. You must rely on 'AI vibes' grounded in real repo signals:\n" +
	"- duplicated logic across tools/backends\n" +
	"- inconsistent CLI contracts / help/docstring standards\n" +
	"- missing shared helpers that should live under backend/idf/\n" +
	"- workflow gaps in Makefile/composite pipelines\n" +
	"- missing regression coverage for brittle logic\n" +
	"\n" +
	"Mandatory scouting (repo_prompt):\n" +
	"- Start by calling get_file_tree.\n" +
	"- Then read a small but representative set of files across backend/, tools/, frontend/, and ops/.\n" +
	"\n" +
	"Queue seeding rule:\n" +
	"- If `## Queue` is empty, you MUST populate it with 10-15 high-leverage, outcome-sized items.\n" +
	"\n" +
	"Evidence requirement for NEW items:\n" +
	"- Each item must cite concrete file paths and what you observed (function/class/pattern), or a concrete Make target/workflow gap.\n" +
	"- Do not invent evidence; only claim what you can point to in the repo.\n"

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
	PrintPrompt      bool
	Stdout           io.Writer
	Stderr           io.Writer
	Stdin            io.Reader
}

// BuildResult captures build outputs.
type BuildResult struct {
	Prompt            string
	PromptPath        string
	EffectiveInnovate bool
}

// FillPrompt loads and fills the prompt template with interactive/innovate placeholders.
func FillPrompt(templatePath string, interactive bool, innovate bool) (string, error) {
	content, err := os.ReadFile(templatePath)
	if err != nil {
		return "", err
	}
	prompt := string(content)
	if !strings.Contains(prompt, "AGENTS.md") {
		return "", fmt.Errorf("Prompt template must reference AGENTS.md (root): %s", templatePath)
	}

	prompt, err = replacePlaceholder(prompt, interactivePlaceholder, interactiveInstructions, interactive)
	if err != nil {
		return "", err
	}
	prompt, err = replacePlaceholder(prompt, innovatePlaceholder, innovateInstructions, innovate)
	if err != nil {
		return "", err
	}

	return prompt, nil
}

// ResolveInnovate applies the autofill scout rules to determine the effective innovate mode.
func ResolveInnovate(queuePath string, innovate bool, innovateExplicit bool, autofillScout bool) (bool, error) {
	if innovateExplicit || !autofillScout {
		return innovate, nil
	}
	count, err := uncheckedQueueCount(queuePath)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return true, nil
		}
		return false, err
	}
	if count == 0 {
		return true, nil
	}
	return innovate, nil
}

// Build runs the specs builder with the given options.
func Build(opts BuildOptions) (BuildResult, error) {
	if opts.PromptTemplate == "" {
		opts.PromptTemplate = filepath.Join(opts.PinDir, "specs_builder.md")
	}
	if opts.Runner == "" {
		opts.Runner = RunnerCodex
	}

	if err := verifyRunner(opts.Runner); err != nil {
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

	prompt, err := FillPrompt(opts.PromptTemplate, opts.Interactive, effectiveInnovate)
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

	if err := runRunner(opts, prompt, promptPath); err != nil {
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
	lockPid := filepath.Join(lockDir, "owner.pid")

	if err := os.Mkdir(lockDir, 0o700); err == nil {
		if err := os.WriteFile(lockPid, []byte(strconv.Itoa(os.Getpid())), 0o600); err != nil {
			return nil, err
		}
		return &Lock{dir: lockDir, acquired: true}, nil
	} else if !os.IsExist(err) {
		return nil, err
	}

	ownerPID, err := os.ReadFile(lockPid)
	if err != nil {
		return nil, fmt.Errorf("Ralph lock exists but owner pid file is missing. Remove %s to clear the lock.", lockDir)
	}
	pidStr := strings.TrimSpace(string(ownerPID))
	if pidStr == "" {
		return nil, fmt.Errorf("Ralph lock exists but owner pid file is missing. Remove %s to clear the lock.", lockDir)
	}

	pid, err := strconv.Atoi(pidStr)
	if err == nil && isAncestorPID(pid) {
		return &Lock{dir: lockDir, acquired: false}, nil
	}

	if err == nil && !isPIDRunning(pid) {
		_ = os.RemoveAll(lockDir)
		if err := os.Mkdir(lockDir, 0o700); err == nil {
			if err := os.WriteFile(lockPid, []byte(strconv.Itoa(os.Getpid())), 0o600); err != nil {
				return nil, err
			}
			return &Lock{dir: lockDir, acquired: true}, nil
		}
	}

	return nil, fmt.Errorf("Another Ralph process is running (lock: %s).", lockDir)
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

func runRunner(opts BuildOptions, prompt string, promptPath string) error {
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
			cmd := exec.Command("codex", append(runArgs, prompt)...)
			cmd.Stdout = stdout
			cmd.Stderr = stderr
			cmd.Stdin = stdin
			if err := cmd.Run(); err != nil {
				return fmt.Errorf("codex failed while building specs.")
			}
			return nil
		}
		args := append([]string{"exec"}, runArgs...)
		args = append(args, "-")
		cmd := exec.Command("codex", args...)
		file, err := os.Open(promptPath)
		if err != nil {
			return err
		}
		defer file.Close()
		cmd.Stdin = file
		cmd.Stdout = stdout
		cmd.Stderr = stderr
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("codex failed while building specs.")
		}
		return nil
	case RunnerOpencode:
		if opts.Interactive {
			cmd := exec.Command("opencode", append(runArgs, prompt)...)
			cmd.Stdout = stdout
			cmd.Stderr = stderr
			cmd.Stdin = stdin
			if err := cmd.Run(); err != nil {
				return fmt.Errorf("opencode failed while building specs.")
			}
			return nil
		}
		args := append([]string{"run"}, runArgs...)
		args = append(args, "--file", promptPath, "--", "Follow the attached prompt file verbatim.")
		cmd := exec.Command("opencode", args...)
		cmd.Stdout = stdout
		cmd.Stderr = stderr
		cmd.Stdin = stdin
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("opencode failed while building specs.")
		}
		return nil
	default:
		return fmt.Errorf("--runner must be codex or opencode (got: %s)", opts.Runner)
	}
}

func verifyRunner(runner Runner) error {
	switch runner {
	case RunnerCodex:
		if _, err := exec.LookPath("codex"); err != nil {
			return fmt.Errorf("codex is not on PATH. Install it or use --runner opencode.")
		}
	case RunnerOpencode:
		if _, err := exec.LookPath("opencode"); err != nil {
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
	cmd := exec.Command("cksum")
	cmd.Stdin = bytes.NewBufferString(repoRoot)
	output, err := cmd.Output()
	if err == nil {
		fields := strings.Fields(string(output))
		if len(fields) > 0 {
			return fields[0]
		}
	}
	return fmt.Sprintf("%x", crc32Checksum(repoRoot))
}

func crc32Checksum(value string) uint32 {
	return crc32Sum([]byte(value))
}

func crc32Sum(data []byte) uint32 {
	var crc uint32 = 0xFFFFFFFF
	for _, b := range data {
		crc ^= uint32(b) << 24
		for i := 0; i < 8; i++ {
			if crc&0x80000000 != 0 {
				crc = (crc << 1) ^ 0x04C11DB7
			} else {
				crc <<= 1
			}
		}
	}
	return crc
}

func isPIDRunning(pid int) bool {
	cmd := exec.Command("ps", "-p", strconv.Itoa(pid))
	cmd.Stdout = nil
	cmd.Stderr = nil
	return cmd.Run() == nil
}

func isAncestorPID(ancestorPID int) bool {
	currentPID := os.Getpid()
	for currentPID > 1 {
		if currentPID == ancestorPID {
			return true
		}
		ppid, err := parentPID(currentPID)
		if err != nil || ppid == 0 {
			return false
		}
		currentPID = ppid
	}
	return false
}

func parentPID(pid int) (int, error) {
	cmd := exec.Command("ps", "-o", "ppid=", "-p", strconv.Itoa(pid))
	output, err := cmd.Output()
	if err != nil {
		return 0, err
	}
	trimmed := strings.TrimSpace(string(output))
	if trimmed == "" {
		return 0, nil
	}
	return strconv.Atoi(trimmed)
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
