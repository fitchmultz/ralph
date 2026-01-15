//go:build windows

// Package procgroup provides no-op process group configuration on Windows.
package procgroup

import "os/exec"

// Configure is a no-op on Windows.
func Configure(cmd *exec.Cmd) {}
