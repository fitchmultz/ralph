// Package tui provides tests for the TUI debug logger.
package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
)

func TestTUILoggerWritesJSONL(t *testing.T) {
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
	logger.Info("test.event", map[string]any{"note": "hello"})

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("read log file: %v", err)
	}
	payload := string(data)
	if !strings.Contains(payload, "\"msg\":\"test.event\"") {
		t.Fatalf("expected log entry in payload, got %q", payload)
	}
}
