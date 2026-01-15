// Package pin provides tests for pin validation and operations.
// Entrypoint: go test ./...
package pin

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/lockfile"
)

type fixturePaths struct {
	pinDir string
	queue  string
	done   string
	lookup string
	readme string
}

func TestValidatePinFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)
	files := ResolveFiles(fixture.pinDir)

	if err := ValidatePin(files); err != nil {
		t.Fatalf("ValidatePin failed: %v", err)
	}
}

func TestInitLayoutCreatesValidPin(t *testing.T) {
	tmpDir := t.TempDir()
	pinDir := filepath.Join(tmpDir, ".ralph", "pin")
	cacheDir := filepath.Join(tmpDir, ".ralph", "cache")

	result, err := InitLayout(pinDir, cacheDir, InitOptions{})
	if err != nil {
		t.Fatalf("InitLayout failed: %v", err)
	}

	files := ResolveFiles(pinDir)
	if err := ValidatePin(files); err != nil {
		t.Fatalf("ValidatePin failed after init: %v", err)
	}

	if _, err := os.Stat(files.SpecsPath); err != nil {
		t.Fatalf("specs_builder missing after init: %v", err)
	}

	cacheInfo, err := os.Stat(result.CacheDir)
	if err != nil {
		t.Fatalf("cache dir missing after init: %v", err)
	}
	if !cacheInfo.IsDir() {
		t.Fatalf("cache path is not a directory: %s", result.CacheDir)
	}
}

func TestReadQueueSummaryFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)

	items, blocked, err := ReadQueueSummary(fixture.queue)
	if err != nil {
		t.Fatalf("ReadQueueSummary failed: %v", err)
	}
	if len(items) != 2 {
		t.Fatalf("expected 2 queue items, got %d", len(items))
	}
	if blocked != 1 {
		t.Fatalf("expected 1 blocked item, got %d", blocked)
	}
}

func TestReadBlockedItemsFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)

	items, err := ReadBlockedItems(fixture.queue)
	if err != nil {
		t.Fatalf("ReadBlockedItems failed: %v", err)
	}
	if len(items) != 1 {
		t.Fatalf("expected 1 blocked item, got %d", len(items))
	}
	item := items[0]
	if item.ID != "RQ-0003" {
		t.Fatalf("expected ID RQ-0003, got %s", item.ID)
	}
	if item.Metadata.WIPBranch == "" || item.Metadata.KnownGood == "" {
		t.Fatalf("expected blocked metadata to include WIP branch and known-good")
	}
	if item.FixupAttempts != 0 {
		t.Fatalf("expected fixup attempts default 0, got %d", item.FixupAttempts)
	}
	if item.FixupLast != "" {
		t.Fatalf("expected empty fixup last, got %q", item.FixupLast)
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
		WIPBranch:   "ralph/wip/RQ-TEST/20260113_000000",
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
		"  - WIP branch: ralph/wip/RQ-TEST/20260113_000000",
		"  - Known-good: deadbeef",
		"  - Unblock hint: run make ci",
	}
	if len(block) < len(expectedTail) {
		t.Fatalf("blocked item missing metadata lines")
	}
	assertSliceEqual(t, block[len(block)-len(expectedTail):], expectedTail)
}

func TestBlockItemMatchesExactID(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	content := strings.ReplaceAll(string(data), "RQ-0001", "RQ-00010")
	content = strings.ReplaceAll(content, "RQ-0002", "RQ-00020")
	content = strings.ReplaceAll(content, "RQ-0003", "RQ-00030")
	content = strings.ReplaceAll(content, "RQ-0004", "RQ-00040")
	if err := os.WriteFile(queuePath, []byte(content), 0o600); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	ok, err := BlockItem(queuePath, "RQ-0001", []string{"reason"}, Metadata{})
	if err != nil {
		t.Fatalf("BlockItem failed: %v", err)
	}
	if ok {
		t.Fatalf("expected block to fail for missing exact ID")
	}
}

func TestToggleQueueItemChecked(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	ok, checked, err := ToggleQueueItemChecked(queuePath, "RQ-0001")
	if err != nil {
		t.Fatalf("ToggleQueueItemChecked failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected toggle to succeed")
	}
	if !checked {
		t.Fatalf("expected item to be checked after toggle")
	}

	items, err := ReadQueueItems(queuePath)
	if err != nil {
		t.Fatalf("ReadQueueItems failed: %v", err)
	}
	if len(items) == 0 {
		t.Fatalf("expected queue items")
	}
	if !items[0].Checked {
		t.Fatalf("expected first item to be checked")
	}

	_, checked, err = ToggleQueueItemChecked(queuePath, "RQ-0001")
	if err != nil {
		t.Fatalf("ToggleQueueItemChecked failed: %v", err)
	}
	if checked {
		t.Fatalf("expected item to be unchecked after second toggle")
	}
}

func TestRequeueBlockedItem(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	ok, err := RequeueBlockedItem(queuePath, "RQ-0003", RequeueOptions{InsertAtTop: true})
	if err != nil {
		t.Fatalf("RequeueBlockedItem failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected requeue to succeed")
	}

	lines := readFileLines(t, queuePath)
	block, section := findItemBlock(lines, "- [ ] RQ-0003 [ops]: Blocked fixture item. (README.md)")
	if section != "Queue" {
		t.Fatalf("expected item in Queue section, got %s", section)
	}
	if len(block) == 0 || !strings.HasPrefix(block[0], "- [ ]") {
		t.Fatalf("expected unchecked queue header, got %v", block)
	}
	for _, line := range block {
		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "- Blocked reason:") ||
			strings.HasPrefix(trimmed, "- WIP branch:") ||
			strings.HasPrefix(trimmed, "- Known-good:") ||
			strings.HasPrefix(trimmed, "- Unblock hint:") {
			t.Fatalf("blocked metadata leaked into requeued item: %s", line)
		}
	}
}

func TestRecordFixupAttempt(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	ok, attempts, err := RecordFixupAttempt(queuePath, "RQ-0003", "2026-01-15T00:00:00Z ci failed")
	if err != nil {
		t.Fatalf("RecordFixupAttempt failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected fixup attempt to update blocked item")
	}
	if attempts != 1 {
		t.Fatalf("expected attempts=1, got %d", attempts)
	}

	lines := readFileLines(t, queuePath)
	block, section := findItemBlock(lines, "- [ ] RQ-0003 [ops]: Blocked fixture item. (README.md)")
	if section != "Blocked" {
		t.Fatalf("expected item in Blocked section, got %s", section)
	}
	foundAttempts := false
	foundLast := false
	for _, line := range block {
		trimmed := strings.TrimLeft(line, " \t")
		if trimmed == "- Fixup attempts: 1" {
			foundAttempts = true
		}
		if strings.HasPrefix(trimmed, "- Fixup last:") {
			foundLast = true
		}
	}
	if !foundAttempts || !foundLast {
		t.Fatalf("expected fixup attempt metadata to be appended")
	}
}

func TestPinLockPreventsWrite(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))
	_ = copyFixture(t, fixture.done, filepath.Join(tmpDir, "implementation_done.md"))

	original, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}

	lock, err := lockfile.Acquire(filepath.Join(filepath.Dir(queuePath), ".lock"), lockfile.AcquireOptions{})
	if err != nil {
		t.Fatalf("acquire lock: %v", err)
	}

	_, _, err = ToggleQueueItemChecked(queuePath, "RQ-0001")
	if err == nil {
		lock.Release()
		t.Fatalf("expected lock error")
	}
	if !strings.Contains(err.Error(), "pin files are locked") {
		lock.Release()
		t.Fatalf("expected pin lock error, got %v", err)
	}

	after, err := os.ReadFile(queuePath)
	if err != nil {
		lock.Release()
		t.Fatalf("read queue after: %v", err)
	}
	if string(after) != string(original) {
		lock.Release()
		t.Fatalf("queue content changed despite lock")
	}

	lock.Release()

	_, checked, err := ToggleQueueItemChecked(queuePath, "RQ-0001")
	if err != nil {
		t.Fatalf("ToggleQueueItemChecked failed after release: %v", err)
	}
	if !checked {
		t.Fatalf("expected item to be checked after release")
	}
}

func TestConcurrentPinMoveCheckedToDone(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))
	donePath := copyFixture(t, fixture.done, filepath.Join(tmpDir, "implementation_done.md"))
	lookupPath := copyFixture(t, fixture.lookup, filepath.Join(tmpDir, "lookup_table.md"))
	readmePath := copyFixture(t, fixture.readme, filepath.Join(tmpDir, "README.md"))

	start := make(chan struct{})
	results := make(chan error, 2)
	var wg sync.WaitGroup

	for i := 0; i < 2; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			_, err := MoveCheckedToDone(queuePath, donePath, true)
			results <- err
		}()
	}

	close(start)
	wg.Wait()
	close(results)

	successes := 0
	failures := 0
	for err := range results {
		if err == nil {
			successes++
			continue
		}
		if !strings.Contains(err.Error(), "pin files are locked") {
			t.Fatalf("unexpected error: %v", err)
		}
		failures++
	}

	if successes != 1 || failures != 1 {
		t.Fatalf("expected 1 success/1 failure, got %d success/%d failure", successes, failures)
	}

	if err := ValidatePin(Files{
		QueuePath:  queuePath,
		DonePath:   donePath,
		LookupPath: lookupPath,
		ReadmePath: readmePath,
	}); err != nil {
		t.Fatalf("ValidatePin failed after concurrent move: %v", err)
	}
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

	done := filepath.Join(pinDir, "implementation_done.md")
	lookup := filepath.Join(pinDir, "lookup_table.md")
	readme := filepath.Join(pinDir, "README.md")
	return fixturePaths{
		pinDir: pinDir,
		queue:  queue,
		done:   done,
		lookup: lookup,
		readme: readme,
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
