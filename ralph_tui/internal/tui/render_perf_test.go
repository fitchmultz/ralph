// Package tui provides performance-oriented benchmarks for log rendering paths.
package tui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
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
	loopPath := loopOutputLogPath(tmpDir)
	if err := os.WriteFile(loopPath, []byte(strings.Join(loopLines, "\n")+"\n"), 0o600); err != nil {
		b.Fatalf("write loop output: %v", err)
	}
	specsPath := specsOutputLogPath(tmpDir)
	if err := os.WriteFile(specsPath, []byte(strings.Join(specsLines, "\n")+"\n"), 0o600); err != nil {
		b.Fatalf("write specs output: %v", err)
	}
	view.SetCacheDir(tmpDir)
	view.Refresh()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		view.Refresh()
	}
}

func BenchmarkLoopViewAppendLogLines(b *testing.B) {
	b.ReportAllocs()

	view := newLoopView(testLoopConfig(), paths.Locations{}, newTestKeyMap())
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
	cfg, err = config.ResolvePaths(cfg, repoRoot, repoRoot)
	if err != nil {
		b.Fatalf("ResolvePaths failed: %v", err)
	}
	if err := cfg.Validate(); err != nil {
		b.Fatalf("validate config: %v", err)
	}
	locs := paths.Locations{
		CWD:      repoRoot,
		RepoRoot: repoRoot,
		HomeDir:  repoRoot,
	}

	view, err := newSpecsView(cfg, locs, newTestKeyMap())
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

func BenchmarkSpecsPreviewSignature(b *testing.B) {
	b.ReportAllocs()

	buffer := newLogLineBuffer(0, 0)
	buffer.AppendLines(makeBenchmarkLines("spec", 5000))
	runSignature := buffer.Signature()
	diffStat := strings.Repeat("1 file changed, 1 insertion(+)\n", 2000) + "2000 files changed, 2000 insertions(+)"
	diffSignature := diffStatSignature(diffStat)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = previewInputSignature(
			80,
			fileStamp{Exists: false},
			fileStamp{Exists: false},
			false,
			false,
			false,
			false,
			false,
			"",
			runSignature,
			diffSignature,
		)
	}
}

func makeBenchmarkLines(prefix string, count int) []string {
	lines := make([]string, count)
	for i := 0; i < count; i++ {
		lines[i] = fmt.Sprintf("%s-%d", prefix, i)
	}
	return lines
}
