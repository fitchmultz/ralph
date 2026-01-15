//go:build !windows

// Package procgroup configures process groups for cancellation on Unix systems.
package procgroup

import (
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
	if cmd.Cancel != nil {
		cmd.Cancel = func() error {
			if cmd.Process == nil {
				return nil
			}
			pgid, err := syscall.Getpgid(cmd.Process.Pid)
			if err != nil {
				return cmd.Process.Kill()
			}
			return syscall.Kill(-pgid, syscall.SIGKILL)
		}
	}
}
