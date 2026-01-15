// Package tui provides tests for the TUI debug logger.
package tui

import (
	"bytes"
	"errors"
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

func TestTUILoggerRotatesOversizedLogOnStartup(t *testing.T) {
	tmpDir := t.TempDir()
	logPath := filepath.Join(tmpDir, "ralph_tui.log")
	oversized := bytes.Repeat([]byte("a"), int(maxLogSizeBytes+10))
	if err := os.WriteFile(logPath, oversized, 0o600); err != nil {
		t.Fatalf("write oversized log: %v", err)
	}

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

	backupPath := logPath + ".1"
	backupInfo, err := os.Stat(backupPath)
	if err != nil {
		t.Fatalf("expected rotated log at %s: %v", backupPath, err)
	}
	if backupInfo.Size() != int64(len(oversized)) {
		t.Fatalf("expected rotated log size %d, got %d", len(oversized), backupInfo.Size())
	}

	logger.Info("post.rotate", nil)

	currentData, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("read current log: %v", err)
	}
	if !strings.Contains(string(currentData), "\"msg\":\"post.rotate\"") {
		t.Fatalf("expected post-rotation log entry in current log")
	}

	backupData, err := os.ReadFile(backupPath)
	if err != nil {
		t.Fatalf("read rotated log: %v", err)
	}
	if strings.Contains(string(backupData), "\"msg\":\"post.rotate\"") {
		t.Fatalf("expected rotated log to remain unchanged after rotation")
	}
}

func TestTUILoggerCapturesWriteErrorAndRecovers(t *testing.T) {
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

	var openCalls int
	var firstFile *os.File
	opts := tuiLoggerOptions{
		OpenFile: func(name string, flag int, perm os.FileMode) (*os.File, error) {
			file, err := os.OpenFile(name, flag, perm)
			if err != nil {
				return nil, err
			}
			openCalls++
			if firstFile == nil {
				firstFile = file
			}
			return file, nil
		},
		Write: func(file *os.File, payload []byte) (int, error) {
			if file == firstFile {
				return 0, errors.New("injected write failure")
			}
			return file.Write(payload)
		},
	}

	logger, err := newTUILoggerWithOptions(cfg, opts)
	if err != nil {
		t.Fatalf("newTUILoggerWithOptions failed: %v", err)
	}
	t.Cleanup(func() {
		_ = logger.Close()
	})

	logger.Info("first.fail", nil)
	if logger.LastError() == nil {
		t.Fatalf("expected write error to be captured")
	}
	if !strings.Contains(logger.LastError().Error(), "injected write failure") {
		t.Fatalf("expected injected write failure, got %v", logger.LastError())
	}

	logger.Info("recover.ok", nil)
	if logger.LastError() != nil {
		t.Fatalf("expected write error to clear after recovery, got %v", logger.LastError())
	}
	if openCalls != 2 {
		t.Fatalf("expected logger to reopen file after failure, got %d opens", openCalls)
	}

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("read log file: %v", err)
	}
	payload := string(data)
	if strings.Contains(payload, "\"msg\":\"first.fail\"") {
		t.Fatalf("expected failed entry to be absent from log, got %q", payload)
	}
	if !strings.Contains(payload, "\"msg\":\"recover.ok\"") {
		t.Fatalf("expected recovered entry in log, got %q", payload)
	}
}

func TestTUILoggerCapturesRotationErrorAndRecovers(t *testing.T) {
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

	var renameCalls int
	opts := tuiLoggerOptions{
		MaxBytes: 1024,
		Rename: func(oldpath, newpath string) error {
			renameCalls++
			if renameCalls == 1 {
				return errors.New("injected rename failure")
			}
			return os.Rename(oldpath, newpath)
		},
	}

	logger, err := newTUILoggerWithOptions(cfg, opts)
	if err != nil {
		t.Fatalf("newTUILoggerWithOptions failed: %v", err)
	}
	t.Cleanup(func() {
		_ = logger.Close()
	})

	oversized := strings.Repeat("a", 5000)
	logger.Info("oversized.event", map[string]any{"blob": oversized})
	if logger.LastError() == nil {
		t.Fatalf("expected rotation error to be captured")
	}
	if !strings.Contains(logger.LastError().Error(), "injected rename failure") {
		t.Fatalf("expected injected rename failure, got %v", logger.LastError())
	}

	logger.Info("post.rotate", nil)
	if logger.LastError() != nil {
		t.Fatalf("expected rotation error to clear after recovery, got %v", logger.LastError())
	}

	backupPath := logPath + ".1"
	backupData, err := os.ReadFile(backupPath)
	if err != nil {
		t.Fatalf("read rotated log: %v", err)
	}
	if !strings.Contains(string(backupData), "\"msg\":\"oversized.event\"") {
		t.Fatalf("expected rotated log to contain oversized entry")
	}

	currentData, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatalf("read current log: %v", err)
	}
	currentPayload := string(currentData)
	if strings.Contains(currentPayload, "\"msg\":\"oversized.event\"") {
		t.Fatalf("expected oversized entry to be rotated out of current log")
	}
	if !strings.Contains(currentPayload, "\"msg\":\"post.rotate\"") {
		t.Fatalf("expected post-rotation entry in current log")
	}
}
