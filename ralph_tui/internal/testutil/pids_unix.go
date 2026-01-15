//go:build !windows

// Package testutil provides test helpers shared across Ralph packages.
package testutil

import (
	"fmt"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// ParseChildPID extracts a CHILD_PID value from output lines.
func ParseChildPID(output string) (int, bool) {
	for _, line := range strings.Split(output, "\n") {
		if strings.HasPrefix(line, "CHILD_PID=") {
			value := strings.TrimPrefix(line, "CHILD_PID=")
			pid, err := strconv.Atoi(strings.TrimSpace(value))
			if err == nil && pid > 0 {
				return pid, true
			}
		}
	}
	return 0, false
}

// WaitForChildPID polls output until a CHILD_PID is observed or times out.
func WaitForChildPID(outputFn func() string, timeout time.Duration) (int, bool) {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if pid, ok := ParseChildPID(outputFn()); ok {
			return pid, true
		}
		time.Sleep(10 * time.Millisecond)
	}
	return 0, false
}

// WaitForPIDExit waits until a process is no longer running or times out.
func WaitForPIDExit(pid int, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if !IsPIDRunning(pid) {
			return nil
		}
		time.Sleep(25 * time.Millisecond)
	}
	return fmt.Errorf("pid %d still running", pid)
}

// IsPIDRunning reports whether a PID exists (best-effort).
func IsPIDRunning(pid int) bool {
	if pid <= 0 {
		return false
	}
	return syscall.Kill(pid, 0) == nil
}
