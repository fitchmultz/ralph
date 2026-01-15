//go:build !windows

// Package tui provides helper process entrypoints for TUI shutdown tests.
package tui

import (
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"testing"
)

const (
	tuiHelperProcessEnv   = "RALPH_TUI_HELPER_PROCESS"
	tuiHelperModeEnv      = "RALPH_TUI_HELPER_MODE"
	tuiHelperModeSpawn    = "spawn_child_sleep"
	tuiHelperChildPIDFile = "RALPH_TUI_CHILD_PID_FILE"
)

func TestMain(m *testing.M) {
	if os.Getenv(tuiHelperProcessEnv) == "1" {
		tuiHelperMain()
		os.Exit(0)
	}
	os.Exit(m.Run())
}

func tuiHelperMain() {
	switch os.Getenv(tuiHelperModeEnv) {
	case tuiHelperModeSpawn:
		spawnChildSleep()
	default:
		_, _ = fmt.Fprintf(os.Stderr, "unknown helper mode: %s\n", os.Getenv(tuiHelperModeEnv))
		os.Exit(2)
	}
}

func spawnChildSleep() {
	if _, err := exec.LookPath("sleep"); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "sleep not found: %v\n", err)
		os.Exit(2)
	}
	cmd := exec.Command("sleep", "30")
	if err := cmd.Start(); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "start sleep: %v\n", err)
		os.Exit(2)
	}
	pid := cmd.Process.Pid
	fmt.Printf("CHILD_PID=%d\n", pid)
	if path := os.Getenv(tuiHelperChildPIDFile); path != "" {
		if err := os.WriteFile(path, []byte(strconv.Itoa(pid)), 0o600); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "write pid file: %v\n", err)
			os.Exit(2)
		}
	}
	_ = cmd.Wait()
}
