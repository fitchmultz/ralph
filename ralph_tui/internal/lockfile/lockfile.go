// Package lockfile provides directory-based locks with pid ownership.
package lockfile

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

const ownerFilename = "owner.pid"

// AcquireOptions controls lock acquisition behavior.
type AcquireOptions struct {
	AllowAncestor bool
}

// Lock represents an acquired lock.
type Lock struct {
	dir      string
	acquired bool
}

// Dir returns the lock directory.
func (l *Lock) Dir() string {
	if l == nil {
		return ""
	}
	return l.dir
}

// Acquired reports whether this process acquired the lock.
func (l *Lock) Acquired() bool {
	if l == nil {
		return false
	}
	return l.acquired
}

// Acquire obtains a lock directory or returns an error if it is held.
func Acquire(lockDir string, opts AcquireOptions) (*Lock, error) {
	cleanDir := filepath.Clean(lockDir)
	ownerPath := filepath.Join(cleanDir, ownerFilename)

	if err := os.Mkdir(cleanDir, 0o700); err == nil {
		if err := writeOwnerPID(ownerPath); err != nil {
			_ = os.RemoveAll(cleanDir)
			return nil, err
		}
		return &Lock{dir: cleanDir, acquired: true}, nil
	} else if !os.IsExist(err) {
		return nil, err
	}

	pid, err := readOwnerPID(ownerPath)
	if err != nil {
		return nil, err
	}
	if opts.AllowAncestor && isAncestorPID(pid) {
		return &Lock{dir: cleanDir, acquired: false}, nil
	}
	if pid == os.Getpid() {
		return nil, fmt.Errorf("Lock is already held by this process (lock: %s).", cleanDir)
	}
	if isPIDRunning(pid) {
		return nil, fmt.Errorf("Another Ralph process is running (lock: %s).", cleanDir)
	}

	_ = os.RemoveAll(cleanDir)
	if err := os.Mkdir(cleanDir, 0o700); err != nil {
		return nil, err
	}
	if err := writeOwnerPID(ownerPath); err != nil {
		_ = os.RemoveAll(cleanDir)
		return nil, err
	}
	return &Lock{dir: cleanDir, acquired: true}, nil
}

// Release frees the lock if this process acquired it.
func (l *Lock) Release() {
	if l == nil || !l.acquired {
		return
	}
	_ = os.RemoveAll(l.dir)
}

func writeOwnerPID(path string) error {
	return os.WriteFile(path, []byte(strconv.Itoa(os.Getpid())), 0o600)
}

func readOwnerPID(path string) (int, error) {
	ownerPID, err := os.ReadFile(path)
	if err != nil {
		return 0, fmt.Errorf("Ralph lock exists but owner pid file is missing. Remove %s to clear the lock.", filepath.Dir(path))
	}
	pidStr := strings.TrimSpace(string(ownerPID))
	if pidStr == "" {
		return 0, fmt.Errorf("Ralph lock exists but owner pid file is missing. Remove %s to clear the lock.", filepath.Dir(path))
	}
	pid, err := strconv.Atoi(pidStr)
	if err != nil {
		return 0, fmt.Errorf("Ralph lock has invalid owner pid. Remove %s to clear the lock.", filepath.Dir(path))
	}
	return pid, nil
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
