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

type lockedBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
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
	default:
		fmt.Fprintln(os.Stderr, "unknown helper mode")
		os.Exit(2)
	}

	os.Exit(0)
}
