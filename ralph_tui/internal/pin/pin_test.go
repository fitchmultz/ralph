// Package pin provides tests for pin validation and operations.
// Entrypoint: go test ./...
package pin

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

type fixturePaths struct {
	repoRoot string
	pinDir   string
	queue    string
	done     string
	lookup   string
	readme   string
	prompt   string
}

func TestValidatePinFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)
	files := ResolveFiles(fixture.pinDir, fixture.repoRoot)

	if err := ValidatePin(files); err != nil {
		t.Fatalf("ValidatePin failed: %v", err)
	}
}

func TestMoveCheckedToDoneFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)

	cases := []struct {
		name    string
		prepend bool
	}{
		{name: "append", prepend: false},
		{name: "prepend", prepend: true},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			tmpDir := t.TempDir()
			queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))
			donePath := copyFixture(t, fixture.done, filepath.Join(tmpDir, "implementation_done.md"))

			items, err := ReadQueueItems(queuePath)
			if err != nil {
				t.Fatalf("ReadQueueItems failed: %v", err)
			}
			movedBlocks := make([][]string, 0)
			for _, item := range items {
				if item.Checked {
					movedBlocks = append(movedBlocks, item.Lines)
				}
			}
			inserted := flattenBlocks(movedBlocks)

			_, err = MoveCheckedToDone(queuePath, donePath, tc.prepend)
			if err != nil {
				t.Fatalf("MoveCheckedToDone failed: %v", err)
			}

			queueItems, err := ReadQueueItems(queuePath)
			if err != nil {
				t.Fatalf("ReadQueueItems failed: %v", err)
			}
			for _, item := range queueItems {
				if item.Checked {
					t.Fatalf("checked item still in queue: %s", item.Header)
				}
			}

			doneLines := readFileLines(t, donePath)
			doneIndex := indexOfLine(doneLines, "## Done")
			if doneIndex == -1 {
				t.Fatalf("Done section missing")
			}

			if len(inserted) > 0 {
				if tc.prepend {
					assertSliceEqual(t, doneLines[doneIndex+1:doneIndex+1+len(inserted)], inserted)
				} else {
					sectionEnd := len(doneLines)
					for i := doneIndex + 1; i < len(doneLines); i++ {
						if strings.HasPrefix(doneLines[i], "## ") {
							sectionEnd = i
							break
						}
					}
					assertSliceEqual(t, doneLines[sectionEnd-len(inserted):sectionEnd], inserted)
				}
			}
		})
	}
}

func TestBlockItemFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	items, err := ReadQueueItems(queuePath)
	if err != nil {
		t.Fatalf("ReadQueueItems failed: %v", err)
	}
	if len(items) == 0 {
		t.Fatalf("expected at least one queue item")
	}
	item := items[0]

	reasons := []string{"blocked for test", "unblock after fix"}
	metadata := Metadata{
		WIPBranch:   "ralph/wip/IDFQ-TEST/20260113_000000",
		KnownGood:   "deadbeef",
		UnblockHint: "run make ci",
	}

	ok, err := BlockItem(queuePath, item.ID, reasons, metadata)
	if err != nil {
		t.Fatalf("BlockItem failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected block to succeed")
	}

	lines := readFileLines(t, queuePath)
	block, section := findItemBlock(lines, item.Header)
	if section != "Blocked" {
		t.Fatalf("expected item in Blocked section, got %s", section)
	}

	expectedTail := []string{
		"  - Blocked reason: blocked for test",
		"  - Blocked reason: unblock after fix",
		"  - WIP branch: ralph/wip/IDFQ-TEST/20260113_000000",
		"  - Known-good: deadbeef",
		"  - Unblock hint: run make ci",
	}
	if len(block) < len(expectedTail) {
		t.Fatalf("blocked item missing metadata lines")
	}
	assertSliceEqual(t, block[len(block)-len(expectedTail):], expectedTail)
}

func mustLocateFixtures(t *testing.T) fixturePaths {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("unable to resolve testdata path")
	}

	baseDir := filepath.Dir(file)
	pinDir := filepath.Join(baseDir, "testdata", "pin")
	queue := filepath.Join(pinDir, "implementation_queue.md")
	if !fileExists(queue) {
		t.Fatalf("unable to locate pin fixtures from %s", pinDir)
	}

	repoRoot := filepath.Clean(filepath.Join(baseDir, "..", "..", ".."))
	done := filepath.Join(pinDir, "implementation_done.md")
	lookup := filepath.Join(pinDir, "lookup_table.md")
	readme := filepath.Join(pinDir, "README.md")
	prompt := filepath.Join(repoRoot, "ralph_legacy", "prompt.md")

	return fixturePaths{
		repoRoot: repoRoot,
		pinDir:   pinDir,
		queue:    queue,
		done:     done,
		lookup:   lookup,
		readme:   readme,
		prompt:   prompt,
	}
}

func copyFixture(t *testing.T, src string, dst string) string {
	t.Helper()
	data, err := os.ReadFile(src)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	if err := os.WriteFile(dst, data, 0o600); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
	return dst
}

func readFileLines(t *testing.T, path string) []string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	content := strings.TrimSuffix(string(data), "\n")
	if content == "" {
		return []string{}
	}
	return strings.Split(content, "\n")
}

func indexOfLine(lines []string, target string) int {
	for i, line := range lines {
		if line == target {
			return i
		}
	}
	return -1
}

func findItemBlock(lines []string, header string) ([]string, string) {
	blocks := splitBlocks(lines)
	section := ""
	for _, block := range blocks {
		if len(block) == 0 {
			continue
		}
		if strings.HasPrefix(block[0], "## ") {
			section = strings.TrimPrefix(block[0], "## ")
			continue
		}
		if block[0] == header {
			return block, section
		}
	}
	return nil, ""
}

func assertSliceEqual(t *testing.T, got []string, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("length mismatch: got %d want %d", len(got), len(want))
	}
	for i := range got {
		if got[i] != want[i] {
			t.Fatalf("index %d mismatch: got %q want %q", i, got[i], want[i])
		}
	}
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir()
}
