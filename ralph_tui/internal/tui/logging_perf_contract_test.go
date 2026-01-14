// Package tui provides performance contract tests for the TUI logger.
package tui

import (
	"path/filepath"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
)

func TestLoggingPerfContractKeepsFileOpen(t *testing.T) {
	tmpDir := t.TempDir()
	logPath := filepath.Join(tmpDir, "ralph_tui.log")
	cfg := config.Config{
		Logging: config.LoggingConfig{
			Level: "debug",
			File:  logPath,
		},
		Paths: config.PathsConfig{
			CacheDir: tmpDir,
		},
	}

	logger, err := newTUILogger(cfg)
	if err != nil {
		t.Fatalf("newTUILogger failed: %v", err)
	}
	t.Cleanup(func() {
		_ = logger.Close()
	})
	logger.maxBytes = 1024 * 1024

	if logger.file == nil {
		t.Fatal("expected log file handle to be open")
	}
	initialHandle := logger.file

	logger.Info("perf.contract.one", nil)
	logger.Info("perf.contract.two", map[string]any{"count": 2})

	if logger.file == nil {
		t.Fatal("expected log file handle to remain open")
	}
	if logger.file != initialHandle {
		t.Fatal("expected logger to reuse open file handle")
	}

	if err := logger.Close(); err != nil {
		t.Fatalf("close logger: %v", err)
	}
	if logger.file != nil {
		t.Fatal("expected log file handle to be nil after close")
	}
}
