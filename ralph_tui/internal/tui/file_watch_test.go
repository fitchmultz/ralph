// Package tui provides tests for file change detection helpers.
package tui

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFileChangedDetectsMissingAndDeletion(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "queue.md")

	stamp, changed, err := fileChanged(path, fileStamp{})
	if err != nil {
		t.Fatalf("fileChanged missing file: %v", err)
	}
	if changed {
		t.Fatalf("expected missing file to be unchanged with empty stamp")
	}
	if stamp.Exists {
		t.Fatalf("expected missing file stamp to be Exists=false")
	}

	if err := os.WriteFile(path, []byte("line\n"), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
	stamp, changed, err = fileChanged(path, stamp)
	if err != nil {
		t.Fatalf("fileChanged after create: %v", err)
	}
	if !changed {
		t.Fatalf("expected create to be detected as change")
	}

	if err := os.Remove(path); err != nil {
		t.Fatalf("remove file: %v", err)
	}
	_, changed, err = fileChanged(path, stamp)
	if err != nil {
		t.Fatalf("fileChanged after delete: %v", err)
	}
	if !changed {
		t.Fatalf("expected delete to be detected as change")
	}
}

func TestFileChangedDetectsSizeAndModtime(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "specs.md")
	baseTime := time.Date(2024, 1, 1, 12, 0, 0, 0, time.UTC)

	if err := os.WriteFile(path, []byte("one"), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
	if err := os.Chtimes(path, baseTime, baseTime); err != nil {
		t.Fatalf("set modtime: %v", err)
	}

	stamp, changed, err := fileChanged(path, fileStamp{})
	if err != nil {
		t.Fatalf("fileChanged initial: %v", err)
	}
	if !changed {
		t.Fatalf("expected initial stat to be a change from empty stamp")
	}

	if err := os.WriteFile(path, []byte("one-two"), 0o600); err != nil {
		t.Fatalf("rewrite file: %v", err)
	}
	if err := os.Chtimes(path, baseTime, baseTime); err != nil {
		t.Fatalf("reset modtime: %v", err)
	}
	next, changed, err := fileChanged(path, stamp)
	if err != nil {
		t.Fatalf("fileChanged size update: %v", err)
	}
	if !changed {
		t.Fatalf("expected size change with same modtime to be detected")
	}

	newTime := baseTime.Add(2 * time.Second)
	if err := os.Chtimes(path, newTime, newTime); err != nil {
		t.Fatalf("bump modtime: %v", err)
	}
	_, changed, err = fileChanged(path, next)
	if err != nil {
		t.Fatalf("fileChanged modtime update: %v", err)
	}
	if !changed {
		t.Fatalf("expected modtime change to be detected")
	}
}

func TestFileChangedDetectsSameSizeSameModtimeContentChange(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "queue.md")
	baseTime := time.Date(2024, 1, 1, 12, 0, 0, 0, time.UTC)

	if err := os.WriteFile(path, []byte("one"), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
	if err := os.Chtimes(path, baseTime, baseTime); err != nil {
		t.Fatalf("set modtime: %v", err)
	}

	stamp, changed, err := fileChanged(path, fileStamp{})
	if err != nil {
		t.Fatalf("fileChanged initial: %v", err)
	}
	if !changed {
		t.Fatalf("expected initial stat to be a change from empty stamp")
	}

	if err := os.WriteFile(path, []byte("two"), 0o600); err != nil {
		t.Fatalf("rewrite file: %v", err)
	}
	if err := os.Chtimes(path, baseTime, baseTime); err != nil {
		t.Fatalf("reset modtime: %v", err)
	}

	_, changed, err = fileChanged(path, stamp)
	if err != nil {
		t.Fatalf("fileChanged same size/modtime update: %v", err)
	}
	if !changed {
		t.Fatalf("expected same size/modtime content change to be detected")
	}
}
