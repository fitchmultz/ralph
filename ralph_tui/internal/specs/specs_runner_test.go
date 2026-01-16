// Package specs provides hermetic runner tests for the specs builder.
package specs

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

type testRunnerBackend struct {
	mode string
}

func (b testRunnerBackend) LookPath(file string) (string, error) {
	return file, nil
}

func (b testRunnerBackend) CommandContext(ctx context.Context, name string, args ...string) *exec.Cmd {
	cmdArgs := append([]string{"-test.run=TestSpecsRunnerHelperProcess", "--"}, args...)
	cmd := exec.CommandContext(ctx, os.Args[0], cmdArgs...)
	cmd.Env = append(os.Environ(),
		"RALPH_SPECS_HELPER=1",
		"RALPH_SPECS_MODE="+b.mode,
	)
	return cmd
}

type failLookPathBackend struct{}

func (failLookPathBackend) LookPath(file string) (string, error) {
	return "", fmt.Errorf("unexpected LookPath for %s", file)
}

func (failLookPathBackend) CommandContext(ctx context.Context, name string, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, name, args...)
}

type lockedBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

type flushBuffer struct {
	mu            sync.Mutex
	buf           bytes.Buffer
	flushed       bytes.Buffer
	flushedCalled bool
}

func (l *lockedBuffer) Write(p []byte) (int, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.buf.Write(p)
}

func (l *lockedBuffer) String() string {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.buf.String()
}

func (f *flushBuffer) Write(p []byte) (int, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.buf.Write(p)
}

func (f *flushBuffer) Flush() {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.flushedCalled = true
	if f.buf.Len() == 0 {
		return
	}
	_, _ = f.flushed.Write(f.buf.Bytes())
	f.buf.Reset()
}

func (f *flushBuffer) FlushedCalled() bool {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.flushedCalled
}

func (f *flushBuffer) String() string {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.buf.Len() == 0 {
		return f.flushed.String()
	}
	return f.flushed.String() + f.buf.String()
}

func writeSpecsPinDir(t *testing.T, template string) string {
	t.Helper()
	dir := t.TempDir()
	templatePath := filepath.Join(dir, "specs_builder.md")
	queuePath := filepath.Join(dir, "implementation_queue.md")
	if err := os.WriteFile(templatePath, []byte(template), 0o600); err != nil {
		t.Fatalf("write template: %v", err)
	}
	queueContent := "## Queue\n- [ ] RQ-0001 [code]: Sample.\n\n## Blocked\n\n## Parking Lot\n"
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}
	return dir
}

func TestBuildRunnerEcho(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nHello from template."
	pinDir := writeSpecsPinDir(t, template)
	var output bytes.Buffer
	_, err := Build(context.Background(), BuildOptions{
		RepoRoot:      pinDir,
		PinDir:        pinDir,
		Runner:        RunnerCodex,
		RunnerBackend: testRunnerBackend{mode: "echo"},
		Stdout:        &output,
		Stderr:        &output,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	if !strings.Contains(output.String(), "Hello from template.") {
		t.Fatalf("expected runner output to include template content")
	}
}

func TestBuildRunnerArgsCodexNonInteractive(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nArg test."
	pinDir := writeSpecsPinDir(t, template)
	var output bytes.Buffer
	_, err := Build(context.Background(), BuildOptions{
		RepoRoot:      pinDir,
		PinDir:        pinDir,
		Runner:        RunnerCodex,
		RunnerArgs:    []string{"-c", "alpha=beta"},
		RunnerBackend: testRunnerBackend{mode: "args"},
		Stdout:        &output,
		Stderr:        &output,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	args := parseArgLines(output.String())
	expected := []string{"exec", "-c", "alpha=beta", "-"}
	if !matchesArgs(args, expected) {
		t.Fatalf("expected args %v, got %v", expected, args)
	}
}

func TestBuildRunnerArgsOpencodeNonInteractive(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nArg test."
	pinDir := writeSpecsPinDir(t, template)
	var output bytes.Buffer
	_, err := Build(context.Background(), BuildOptions{
		RepoRoot:      pinDir,
		PinDir:        pinDir,
		Runner:        RunnerOpencode,
		RunnerArgs:    []string{"--model", "test"},
		RunnerBackend: testRunnerBackend{mode: "args"},
		Stdout:        &output,
		Stderr:        &output,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	args := parseArgLines(output.String())
	if len(args) < 6 {
		t.Fatalf("expected opencode args, got %v", args)
	}
	if args[0] != "run" {
		t.Fatalf("expected opencode run subcommand, got %q", args[0])
	}
	if args[1] != "--model" || args[2] != "test" {
		t.Fatalf("expected runner args preserved, got %v", args)
	}
	fileIndex := indexOf(args, "--file")
	if fileIndex == -1 || fileIndex+3 >= len(args) {
		t.Fatalf("expected --file with path and -- separator, got %v", args)
	}
	if strings.TrimSpace(args[fileIndex+1]) == "" {
		t.Fatalf("expected prompt path after --file, got %v", args)
	}
	if args[fileIndex+2] != "--" {
		t.Fatalf("expected -- separator after prompt path, got %v", args)
	}
	if args[fileIndex+3] != "Follow the attached prompt file verbatim." {
		t.Fatalf("expected prompt file message, got %v", args)
	}
}

func TestVerifyRunnerNormalizesInput(t *testing.T) {
	backend := testRunnerBackend{mode: "echo"}
	if err := verifyRunner(backend, Runner(" Codex ")); err != nil {
		t.Fatalf("expected codex runner to validate, got %v", err)
	}
	if err := verifyRunner(backend, Runner(" OPENcode ")); err != nil {
		t.Fatalf("expected opencode runner to validate, got %v", err)
	}
}

func TestBuildRunnerFlushesStreamingWriter(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nFlush test."
	pinDir := writeSpecsPinDir(t, template)
	output := &flushBuffer{}
	_, err := Build(context.Background(), BuildOptions{
		RepoRoot:      pinDir,
		PinDir:        pinDir,
		Runner:        RunnerCodex,
		RunnerBackend: testRunnerBackend{mode: "partial"},
		Stdout:        output,
		Stderr:        output,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	if !output.FlushedCalled() {
		t.Fatalf("expected writer Flush to be called")
	}
	if !strings.Contains(output.String(), "partial") {
		t.Fatalf("expected flushed output to include partial line, got %q", output.String())
	}
}

func TestBuildPrintPromptSkipsRunnerVerification(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nHello from template."
	pinDir := writeSpecsPinDir(t, template)
	result, err := Build(context.Background(), BuildOptions{
		RepoRoot:      pinDir,
		PinDir:        pinDir,
		Runner:        RunnerCodex,
		RunnerBackend: failLookPathBackend{},
		PrintPrompt:   true,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	if !strings.Contains(result.Prompt, "Hello from template.") {
		t.Fatalf("expected prompt to include template content")
	}
}

func TestBuildPrintPromptSkipsLockAcquisition(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nLock test."
	pinDir := writeSpecsPinDir(t, template)
	lockBase := t.TempDir()
	t.Setenv("TMPDIR", lockBase)

	cmd := exec.Command(os.Args[0], "-test.run=TestSpecsRunnerHelperProcess", "--")
	cmd.Env = append(os.Environ(),
		"RALPH_SPECS_HELPER=1",
		"RALPH_SPECS_MODE=sleep",
	)
	if err := cmd.Start(); err != nil {
		t.Fatalf("start helper: %v", err)
	}
	defer func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	}()

	lockDir := filepath.Join(strings.TrimRight(lockBase, string(os.PathSeparator)), fmt.Sprintf("ralph.lock.%s", lockChecksum(pinDir)))
	if err := os.MkdirAll(lockDir, 0o700); err != nil {
		t.Fatalf("create lock dir: %v", err)
	}
	ownerPath := filepath.Join(lockDir, "owner.pid")
	if err := os.WriteFile(ownerPath, []byte(strconv.Itoa(cmd.Process.Pid)), 0o600); err != nil {
		t.Fatalf("write owner pid: %v", err)
	}

	result, err := Build(context.Background(), BuildOptions{
		RepoRoot:      pinDir,
		PinDir:        pinDir,
		Runner:        RunnerCodex,
		RunnerBackend: failLookPathBackend{},
		PrintPrompt:   true,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	if !strings.Contains(result.Prompt, "Lock test.") {
		t.Fatalf("expected prompt to include template content")
	}
}

func TestBuildRunnerCancel(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nCancel test."
	pinDir := writeSpecsPinDir(t, template)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	errCh := make(chan error, 1)
	go func() {
		_, err := Build(ctx, BuildOptions{
			RepoRoot:      pinDir,
			PinDir:        pinDir,
			Runner:        RunnerCodex,
			RunnerBackend: testRunnerBackend{mode: "sleep"},
			Stdout:        io.Discard,
			Stderr:        io.Discard,
		})
		errCh <- err
	}()

	time.Sleep(150 * time.Millisecond)
	cancel()

	select {
	case err := <-errCh:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("expected context.Canceled, got %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatalf("timeout waiting for build cancellation")
	}
}

func TestSpecsRunnerHelperProcess(t *testing.T) {
	if os.Getenv("RALPH_SPECS_HELPER") != "1" {
		return
	}

	mode := os.Getenv("RALPH_SPECS_MODE")
	switch mode {
	case "echo":
		if _, err := io.Copy(os.Stdout, os.Stdin); err != nil {
			fmt.Fprintln(os.Stderr, err.Error())
			os.Exit(1)
		}
	case "sleep":
		time.Sleep(10 * time.Second)
	case "parent":
		cmd := exec.Command(os.Args[0], "-test.run=TestSpecsRunnerHelperProcess", "--", "child")
		cmd.Env = append(os.Environ(),
			"RALPH_SPECS_HELPER=1",
			"RALPH_SPECS_MODE=child",
		)
		if err := cmd.Start(); err != nil {
			fmt.Fprintln(os.Stderr, err.Error())
			os.Exit(1)
		}
		fmt.Fprintf(os.Stdout, "CHILD_PID=%d\n", cmd.Process.Pid)
		time.Sleep(10 * time.Second)
	case "child":
		time.Sleep(10 * time.Second)
	case "args":
		args := argsAfterDelimiter("--", os.Args)
		for _, arg := range args {
			fmt.Fprintf(os.Stdout, "ARG=%s\n", arg)
		}
	case "partial":
		if _, err := os.Stdout.WriteString("partial"); err != nil {
			fmt.Fprintln(os.Stderr, err.Error())
			os.Exit(1)
		}
	default:
		fmt.Fprintln(os.Stderr, "unknown helper mode")
		os.Exit(2)
	}

	os.Exit(0)
}

func parseArgLines(output string) []string {
	lines := strings.Split(strings.TrimSpace(output), "\n")
	args := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		const prefix = "ARG="
		if strings.HasPrefix(line, prefix) {
			args = append(args, strings.TrimPrefix(line, prefix))
		}
	}
	return args
}

func matchesArgs(got []string, expected []string) bool {
	if len(got) != len(expected) {
		return false
	}
	for idx, value := range expected {
		if got[idx] != value {
			return false
		}
	}
	return true
}

func indexOf(values []string, target string) int {
	for idx, value := range values {
		if value == target {
			return idx
		}
	}
	return -1
}

func argsAfterDelimiter(delimiter string, args []string) []string {
	for idx, value := range args {
		if value == delimiter {
			if idx+1 < len(args) {
				return args[idx+1:]
			}
			return nil
		}
	}
	return nil
}
