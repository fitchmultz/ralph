//go:build !windows

// Package tui verifies shutdown behavior for loop runner process groups.
package tui

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/mitchfultz/ralph/ralph_tui/internal/testutil"
)

func TestTUIQuitTerminatesRunnerProcessGroup(t *testing.T) {
	if _, err := exec.LookPath("sleep"); err != nil {
		t.Skipf("sleep not available: %v", err)
	}

	pidFile := filepath.Join(t.TempDir(), "child.pid")
	binDir := t.TempDir()
	codexPath := filepath.Join(binDir, "codex")
	if err := os.Symlink(os.Args[0], codexPath); err != nil {
		t.Fatalf("symlink codex helper: %v", err)
	}

	t.Setenv("PATH", fmt.Sprintf("%s:%s", binDir, os.Getenv("PATH")))
	t.Setenv(tuiHelperProcessEnv, "1")
	t.Setenv(tuiHelperModeEnv, tuiHelperModeSpawn)
	t.Setenv(tuiHelperChildPIDFile, pidFile)
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	m, _, _ := newHermeticModel(t)
	m.screen = screenRunLoop
	m.navCollapsed = true
	m.navFocused = false
	m.width = 120
	m.height = 40
	m.applyFocus()
	m.relayout()

	reader, writer := io.Pipe()
	program := tea.NewProgram(m, tea.WithInput(reader), tea.WithOutput(io.Discard), tea.WithoutRenderer())
	errCh := make(chan error, 1)
	go func() {
		_, err := program.Run()
		errCh <- err
	}()

	if _, err := writer.Write([]byte("c")); err != nil {
		t.Fatalf("send run key: %v", err)
	}

	pid, err := waitForPIDFile(pidFile, 5*time.Second)
	if err != nil {
		t.Fatalf("wait for child pid: %v", err)
	}
	if !testutil.IsPIDRunning(pid) {
		t.Fatalf("expected child pid %d to be running", pid)
	}

	if _, err := writer.Write([]byte("q")); err != nil {
		t.Fatalf("send quit key: %v", err)
	}
	_ = writer.Close()

	select {
	case err := <-errCh:
		if err != nil {
			t.Fatalf("program run error: %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("program did not exit after quit")
	}

	if err := testutil.WaitForPIDExit(pid, 2*time.Second); err != nil {
		_ = syscall.Kill(pid, syscall.SIGKILL)
		t.Fatalf("child process still running after quit: %v", err)
	}
}

func waitForPIDFile(path string, timeout time.Duration) (int, error) {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		data, err := os.ReadFile(path)
		if err == nil {
			value := strings.TrimSpace(string(data))
			pid, convErr := strconv.Atoi(value)
			if convErr == nil && pid > 0 {
				return pid, nil
			}
		}
		time.Sleep(20 * time.Millisecond)
	}
	return 0, fmt.Errorf("pid file not populated within %s", timeout)
}
