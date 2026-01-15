// Package tui provides performance-oriented benchmarks for log rendering paths.
package tui

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
)

func BenchmarkLogsViewRefresh_NoChanges(b *testing.B) {
	b.ReportAllocs()

	tmpDir := b.TempDir()
	logPath := filepath.Join(tmpDir, "ralph_tui.log")
	if err := os.WriteFile(logPath, []byte("first\nsecond\n"), 0o600); err != nil {
		b.Fatalf("write log file: %v", err)
	}

	view := newLogsView(logPath)
	loopLines := makeBenchmarkLines("loop", 2000)
	specsLines := makeBenchmarkLines("spec", 500)
	view.Refresh(loopLines, specsLines)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		view.Refresh(loopLines, specsLines)
	}
}

func BenchmarkLoopViewAppendLogLines(b *testing.B) {
	b.ReportAllocs()

	view := newLoopView(testLoopConfig(), paths.Locations{})
	view.mode = loopRunning
	batch := makeBenchmarkLines("loop", 64)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		view.appendLogLines(batch)
	}
}

func BenchmarkSpecsViewAppendRunLogs(b *testing.B) {
	b.ReportAllocs()

	cfg, err := config.DefaultConfig()
	if err != nil {
		b.Fatalf("default config: %v", err)
	}
	repoRoot := b.TempDir()
	cfg = config.ResolvePaths(cfg, repoRoot)
	if err := cfg.Validate(); err != nil {
		b.Fatalf("validate config: %v", err)
	}
	locs := paths.Locations{
		CWD:      repoRoot,
		RepoRoot: repoRoot,
		HomeDir:  repoRoot,
	}

	view, err := newSpecsView(cfg, locs)
	if err != nil {
		b.Fatalf("newSpecsView failed: %v", err)
	}
	view.running = true
	batch := makeBenchmarkLines("spec", 32)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		view.appendRunLogs(batch)
	}
}

func makeBenchmarkLines(prefix string, count int) []string {
	lines := make([]string, count)
	for i := 0; i < count; i++ {
		lines[i] = fmt.Sprintf("%s-%d", prefix, i)
	}
	return lines
}
