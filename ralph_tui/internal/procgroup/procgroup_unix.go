//go:build !windows

// Package procgroup configures process groups for cancellation on Unix systems.
package procgroup

import (
	"fmt"
	"os/exec"
	"syscall"
)

// Configure sets up process-group cancellation so context cancel kills the group.
func Configure(cmd *exec.Cmd) {
	if cmd == nil {
		return
	}
	if cmd.SysProcAttr == nil {
		cmd.SysProcAttr = &syscall.SysProcAttr{}
	}
	cmd.SysProcAttr.Setpgid = true
	originalCancel := cmd.Cancel
	cmd.Cancel = func() error {
		var originalErr error
		if originalCancel != nil {
			originalErr = originalCancel()
		}
		if cmd.Process == nil {
			return originalErr
		}
		pgid, err := syscall.Getpgid(cmd.Process.Pid)
		if err != nil {
			killErr := cmd.Process.Kill()
			if killErr != nil {
				if originalErr != nil {
					return fmt.Errorf("cancel failed: %v; %w", originalErr, killErr)
				}
				return killErr
			}
			return originalErr
		}
		killErr := syscall.Kill(-pgid, syscall.SIGKILL)
		if killErr != nil {
			if originalErr != nil {
				return fmt.Errorf("cancel failed: %v; %w", originalErr, killErr)
			}
			return killErr
		}
		return originalErr
	}
}
