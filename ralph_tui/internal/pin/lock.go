// Package pin provides validation and deterministic operations for Ralph pin files.
package pin

import (
	"fmt"
	"path/filepath"

	"github.com/mitchfultz/ralph/ralph_tui/internal/lockfile"
)

func acquirePinLock(pinDir string) (*lockfile.Lock, error) {
	lockDir := filepath.Join(filepath.Clean(pinDir), ".lock")
	lock, err := lockfile.Acquire(lockDir, lockfile.AcquireOptions{})
	if err != nil {
		return nil, fmt.Errorf("pin files are locked: %w", err)
	}
	return lock, nil
}
