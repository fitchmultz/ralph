// Package tui provides a lightweight JSONL file logger for the TUI.
package tui

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
)

type logLevel int

const (
	logDebug logLevel = iota
	logInfo
	logWarn
	logError
)

const maxLogSizeBytes int64 = 2 * 1024 * 1024

type logEntry struct {
	Timestamp string         `json:"ts"`
	Level     string         `json:"level"`
	Message   string         `json:"msg"`
	Fields    map[string]any `json:"fields,omitempty"`
}

type tuiLoggerOptions struct {
	MaxBytes int64

	OpenFile func(name string, flag int, perm os.FileMode) (*os.File, error)
	Write    func(file *os.File, payload []byte) (int, error)
	Stat     func(name string) (os.FileInfo, error)
	Rename   func(oldpath, newpath string) error
	Remove   func(name string) error
}

type tuiLogger struct {
	path     string
	level    logLevel
	maxBytes int64
	file     *os.File
	fileSize int64
	lastErr  error
	mu       sync.Mutex

	openFileFn func(name string, flag int, perm os.FileMode) (*os.File, error)
	writeFn    func(file *os.File, payload []byte) (int, error)
	statFn     func(name string) (os.FileInfo, error)
	renameFn   func(oldpath, newpath string) error
	removeFn   func(name string) error
}

func newTUILogger(cfg config.Config) (*tuiLogger, error) {
	return newTUILoggerWithOptions(cfg, tuiLoggerOptions{})
}

func newTUILoggerWithOptions(cfg config.Config, opts tuiLoggerOptions) (*tuiLogger, error) {
	path, err := resolveLogPath(cfg)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}

	maxBytes := opts.MaxBytes
	if maxBytes <= 0 {
		maxBytes = maxLogSizeBytes
	}

	openFileFn := opts.OpenFile
	if openFileFn == nil {
		openFileFn = os.OpenFile
	}
	writeFn := opts.Write
	if writeFn == nil {
		writeFn = func(file *os.File, payload []byte) (int, error) {
			return file.Write(payload)
		}
	}
	statFn := opts.Stat
	if statFn == nil {
		statFn = os.Stat
	}
	renameFn := opts.Rename
	if renameFn == nil {
		renameFn = os.Rename
	}
	removeFn := opts.Remove
	if removeFn == nil {
		removeFn = os.Remove
	}

	logger := &tuiLogger{
		path:       path,
		level:      parseLogLevel(cfg.Logging.Level),
		maxBytes:   maxBytes,
		openFileFn: openFileFn,
		writeFn:    writeFn,
		statFn:     statFn,
		renameFn:   renameFn,
		removeFn:   removeFn,
	}
	if err := logger.openFile(); err != nil {
		return nil, err
	}

	return logger, nil
}

func resolveLogPath(cfg config.Config) (string, error) {
	if strings.TrimSpace(cfg.Logging.File) != "" {
		return filepath.Clean(cfg.Logging.File), nil
	}
	if strings.TrimSpace(cfg.Paths.CacheDir) == "" {
		return "", fmt.Errorf("cache dir is required to resolve log path")
	}
	return filepath.Join(cfg.Paths.CacheDir, "ralph_tui.log"), nil
}

func (l *tuiLogger) Path() string {
	if l == nil {
		return ""
	}
	return l.path
}

func (l *tuiLogger) LastError() error {
	if l == nil {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.lastErr
}

func (l *tuiLogger) Debug(message string, fields map[string]any) {
	l.Log(logDebug, message, fields)
}

func (l *tuiLogger) Info(message string, fields map[string]any) {
	l.Log(logInfo, message, fields)
}

func (l *tuiLogger) Warn(message string, fields map[string]any) {
	l.Log(logWarn, message, fields)
}

func (l *tuiLogger) Error(message string, fields map[string]any) {
	l.Log(logError, message, fields)
}

func (l *tuiLogger) Log(level logLevel, message string, fields map[string]any) {
	if l == nil || level < l.level {
		return
	}

	entry := logEntry{
		Timestamp: time.Now().UTC().Format(time.RFC3339Nano),
		Level:     level.String(),
		Message:   message,
		Fields:    fields,
	}

	payload, err := json.Marshal(entry)
	if err != nil {
		return
	}
	payload = append(payload, '\n')

	l.mu.Lock()
	defer l.mu.Unlock()
	if err := l.ensureFileLocked(); err != nil {
		l.setLastErrorLocked(fmt.Errorf("log open/rotate: %w", err))
		_ = l.closeFileLocked()
		return
	}
	if l.file == nil {
		l.setLastErrorLocked(errors.New("log file unavailable"))
		return
	}
	written, err := l.writeFn(l.file, payload)
	if err != nil {
		l.setLastErrorLocked(fmt.Errorf("log write: %w", err))
		_ = l.closeFileLocked()
		return
	}
	if written != len(payload) {
		l.setLastErrorLocked(fmt.Errorf("log write: %w", io.ErrShortWrite))
		_ = l.closeFileLocked()
		return
	}
	l.fileSize += int64(written)
	if err := l.rotateIfNeededLocked(); err != nil {
		l.setLastErrorLocked(fmt.Errorf("log rotate: %w", err))
		_ = l.closeFileLocked()
		return
	}
	l.clearLastErrorLocked()
}

func (l *tuiLogger) Close() error {
	if l == nil {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.closeFileLocked()
}

func (l *tuiLogger) openFile() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.openFileLocked()
}

func (l *tuiLogger) openFileLocked() error {
	if l.file != nil {
		return nil
	}
	if err := l.rotateIfNeededLocked(); err != nil {
		return err
	}
	file, err := l.openFileFn(l.path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	info, err := file.Stat()
	if err != nil {
		_ = file.Close()
		return err
	}
	l.file = file
	l.fileSize = info.Size()
	return nil
}

func (l *tuiLogger) closeFileLocked() error {
	if l.file == nil {
		return nil
	}
	err := l.file.Close()
	l.file = nil
	l.fileSize = 0
	return err
}

func (l *tuiLogger) ensureFileLocked() error {
	if l.file != nil {
		if _, err := l.statFn(l.path); errors.Is(err, os.ErrNotExist) {
			_ = l.closeFileLocked()
		} else if err != nil {
			return err
		}
	}
	if l.file == nil {
		return l.openFileLocked()
	}
	if err := l.rotateIfNeededLocked(); err != nil {
		return err
	}
	if l.file == nil {
		return l.openFileLocked()
	}
	return nil
}

func (l *tuiLogger) rotateIfNeededLocked() error {
	if l.file != nil {
		if l.fileSize < l.maxBytes {
			return nil
		}
	} else {
		info, err := l.statFn(l.path)
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				return nil
			}
			return err
		}
		l.fileSize = info.Size()
		if l.fileSize < l.maxBytes {
			return nil
		}
	}
	if err := l.closeFileLocked(); err != nil {
		return err
	}
	backup := l.path + ".1"
	if err := l.removeFn(backup); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := l.renameFn(l.path, backup); err != nil {
		return err
	}
	l.fileSize = 0
	return nil
}

func (l *tuiLogger) setLastErrorLocked(err error) {
	l.lastErr = err
}

func (l *tuiLogger) clearLastErrorLocked() {
	l.lastErr = nil
}

func parseLogLevel(level string) logLevel {
	switch strings.ToLower(strings.TrimSpace(level)) {
	case "debug":
		return logDebug
	case "warn":
		return logWarn
	case "error":
		return logError
	default:
		return logInfo
	}
}

func (l logLevel) String() string {
	switch l {
	case logDebug:
		return "debug"
	case logWarn:
		return "warn"
	case logError:
		return "error"
	default:
		return "info"
	}
}
