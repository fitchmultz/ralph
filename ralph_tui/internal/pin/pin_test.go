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
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

type fixturePaths struct {
	pinDir    string
	queue     string
	done      string
	lookup    string
	readme    string
	specsCode string
	specsDocs string
}

func TestValidatePinFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)
	files := ResolveFiles(fixture.pinDir)

	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed: %v", err)
	}
}

func TestValidatePinAllowsEmptyQueueAndDone(t *testing.T) {
	fixture := mustLocateEmptyFixtures(t)
	files := ResolveFiles(fixture.pinDir)

	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed for empty fixtures: %v", err)
	}
}

func TestValidatePinAllowsExtraMetadata(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}

	extra := "  - Plan: Keep the fixture in sync with queue validation rules.\n" +
		"  - Notes: Extra context for the fixture.\n" +
		"    - Link: https://example.com/queue-item\n" +
		"  ```yaml\n" +
		"  owner: ralph-team\n" +
		"  severity: medium\n" +
		"  ```"
	updated := strings.Replace(string(data), "  - Plan: Keep the fixture in sync with queue validation rules.", extra, 1)
	if updated == string(data) {
		t.Fatalf("failed to insert extra metadata into fixture")
	}
	if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed with extra metadata: %v", err)
	}
}

func TestValidatePinRejectsUnsafeMetadataLines(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}

	updated := strings.Replace(string(data), "  - Plan: Keep the fixture in sync with queue validation rules.", "  - Plan: Keep the fixture in sync with queue validation rules.\n- [link]: https://example.com/unsafe", 1)
	if updated == string(data) {
		t.Fatalf("failed to insert unsafe metadata into fixture")
	}
	if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	err = ValidatePin(files, project.TypeCode)
	if err == nil {
		t.Fatalf("expected ValidatePin to fail with unsafe metadata")
	}
	if !strings.Contains(err.Error(), "Unindented metadata bullets") {
		t.Fatalf("expected unsafe metadata error, got: %v", err)
	}
}

func TestValidatePinRejectsUnindentedEvidenceOrPlanBullets(t *testing.T) {
	fixture := mustLocateFixtures(t)

	cases := []struct {
		name        string
		needle      string
		replacement string
		expected    string
	}{
		{
			name:        "evidence",
			needle:      "  - Evidence:",
			replacement: "- Evidence:",
			expected:    "Missing indented metadata bullet: \"- Evidence:",
		},
		{
			name:        "plan",
			needle:      "  - Plan:",
			replacement: "- Plan:",
			expected:    "Missing indented metadata bullet: \"- Plan:",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			tmpDir := t.TempDir()
			files := ResolveFiles(tmpDir)
			queuePath := copyFixture(t, fixture.queue, files.QueuePath)
			_ = copyFixture(t, fixture.done, files.DonePath)
			_ = copyFixture(t, fixture.lookup, files.LookupPath)
			_ = copyFixture(t, fixture.readme, files.ReadmePath)
			copyFixtureSpecs(t, fixture, files)

			data, err := os.ReadFile(queuePath)
			if err != nil {
				t.Fatalf("read queue: %v", err)
			}

			updated := strings.Replace(string(data), tc.needle, tc.replacement, 1)
			if updated == string(data) {
				t.Fatalf("failed to replace %q", tc.needle)
			}
			if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
				t.Fatalf("write queue: %v", err)
			}

			err = ValidatePin(files, project.TypeCode)
			if err == nil {
				t.Fatalf("expected ValidatePin to fail with unindented %s metadata", tc.name)
			}
			if !strings.Contains(err.Error(), tc.expected) {
				t.Fatalf("expected missing metadata error, got: %v", err)
			}
			if !strings.Contains(err.Error(), "Unindented metadata bullets") {
				t.Fatalf("expected unindented metadata error, got: %v", err)
			}
		})
	}
}

func TestValidatePinAcceptsUppercaseTags(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}

	updated := strings.Replace(string(data), "[code]", "[CODE]", 1)
	if updated == string(data) {
		t.Fatalf("failed to uppercase routing tag in fixture")
	}
	if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed with uppercase tags: %v", err)
	}
}

func TestValidatePinAcceptsTagSuffixes(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}

	updated := strings.Replace(string(data), "[code]", "[code-refactor]", 1)
	updated = strings.Replace(updated, "[docs]", "[docs-compliance]", 1)
	if updated == string(data) {
		t.Fatalf("failed to update routing tags in fixture")
	}
	if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed with tag suffixes: %v", err)
	}
}

func TestExtractTags(t *testing.T) {
	header := "- [ ] RQ-0001 [code] [ui]: Example"
	tags := ExtractTags(header)
	if len(tags) != 2 {
		t.Fatalf("expected 2 tags, got %#v", tags)
	}
	if tags[0] != "code" || tags[1] != "ui" {
		t.Fatalf("unexpected tags: %#v", tags)
	}
}

func TestExtractTagsCaseInsensitive(t *testing.T) {
	header := "- [ ] RQ-0001 [CODE] [Ui]: Example"
	tags := ExtractTags(header)
	if len(tags) != 2 {
		t.Fatalf("expected 2 tags, got %#v", tags)
	}
	if tags[0] != "code" || tags[1] != "ui" {
		t.Fatalf("unexpected tags: %#v", tags)
	}
}

func TestParseTagList(t *testing.T) {
	result := ParseTagList(" ui, [code-refactor] docs-compliance,unknown,[bad] ")
	if len(result.Tags) != 3 {
		t.Fatalf("expected 3 tags, got %#v", result.Tags)
	}
	if result.Tags[0] != "ui" || result.Tags[1] != "code-refactor" || result.Tags[2] != "docs-compliance" {
		t.Fatalf("unexpected tags: %#v", result.Tags)
	}
	if len(result.Unknown) != 2 {
		t.Fatalf("expected 2 unknown tags, got %#v", result.Unknown)
	}
	if result.Unknown[0] != "unknown" || result.Unknown[1] != "bad" {
		t.Fatalf("unexpected unknown tags: %#v", result.Unknown)
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
	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed after init: %v", err)
	}

	if _, err := os.Stat(files.SpecsBuilderCodePath); err != nil {
		t.Fatalf("specs_builder missing after init: %v", err)
	}
	if _, err := os.Stat(files.SpecsBuilderDocsPath); err != nil {
		t.Fatalf("specs_builder_docs missing after init: %v", err)
	}

	cacheInfo, err := os.Stat(result.CacheDir)
	if err != nil {
		t.Fatalf("cache dir missing after init: %v", err)
	}
	if !cacheInfo.IsDir() {
		t.Fatalf("cache path is not a directory: %s", result.CacheDir)
	}
}

func TestInitLayoutCreatesDocsTemplate(t *testing.T) {
	tmpDir := t.TempDir()
	pinDir := filepath.Join(tmpDir, ".ralph", "pin")
	cacheDir := filepath.Join(tmpDir, ".ralph", "cache")

	_, err := InitLayout(pinDir, cacheDir, InitOptions{ProjectType: project.TypeDocs})
	if err != nil {
		t.Fatalf("InitLayout failed: %v", err)
	}

	files := ResolveFiles(pinDir)
	if _, err := os.Stat(files.SpecsBuilderDocsPath); err != nil {
		t.Fatalf("specs_builder_docs missing after init: %v", err)
	}
	if _, err := os.Stat(files.SpecsBuilderCodePath); err != nil {
		t.Fatalf("specs_builder missing after docs init: %v", err)
	}
	if err := ValidatePin(files, project.TypeDocs); err != nil {
		t.Fatalf("ValidatePin failed for docs init: %v", err)
	}
}

func TestValidatePinDocsRequiresDocsTemplate(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	_ = copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	_ = copyFixture(t, fixture.specsCode, files.SpecsBuilderCodePath)

	err := ValidatePin(files, project.TypeDocs)
	if err == nil {
		t.Fatalf("expected ValidatePin to require docs template")
	}
	if !strings.Contains(err.Error(), SpecsBuilderDocsFilename) {
		t.Fatalf("expected missing docs template error, got: %v", err)
	}
}

func TestMissingFilesIncludesSpecsTemplatesForCodeProject(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	_ = copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)

	missing, err := MissingFiles(files, project.TypeCode)
	if err != nil {
		t.Fatalf("MissingFiles failed: %v", err)
	}
	if len(missing) != 2 {
		t.Fatalf("expected 2 missing templates, got %d (%v)", len(missing), missing)
	}
	missingSet := make(map[string]struct{}, len(missing))
	for _, path := range missing {
		missingSet[path] = struct{}{}
	}
	for _, expected := range []string{files.SpecsBuilderCodePath, files.SpecsBuilderDocsPath} {
		if _, ok := missingSet[expected]; !ok {
			t.Fatalf("expected missing template %s, got %v", expected, missing)
		}
	}
}

func TestMissingFilesIncludesSpecsTemplatesForDocsProject(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	_ = copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)

	missing, err := MissingFiles(files, project.TypeDocs)
	if err != nil {
		t.Fatalf("MissingFiles failed: %v", err)
	}
	if len(missing) != 2 {
		t.Fatalf("expected 2 missing templates, got %d (%v)", len(missing), missing)
	}
	missingSet := make(map[string]struct{}, len(missing))
	for _, path := range missing {
		missingSet[path] = struct{}{}
	}
	for _, expected := range []string{files.SpecsBuilderCodePath, files.SpecsBuilderDocsPath} {
		if _, ok := missingSet[expected]; !ok {
			t.Fatalf("expected missing template %s, got %v", expected, missing)
		}
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

func TestReadQueueItemsRecognizesUppercaseXAsChecked(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	content := strings.Join([]string{
		"## Queue",
		"- [X] RQ-0001 [code]: Uppercase check. (x)",
		"  - Evidence: uppercase checkbox should parse.",
		"  - Plan: ensure parser recognizes [X].",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	if err := os.WriteFile(queuePath, []byte(content), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	items, err := ReadQueueItems(queuePath)
	if err != nil {
		t.Fatalf("ReadQueueItems failed: %v", err)
	}
	if len(items) != 1 {
		t.Fatalf("expected 1 queue item, got %d", len(items))
	}
	if !items[0].Checked {
		t.Fatalf("expected uppercase [X] to be checked")
	}
}

func TestReadDoneSummaryFixtures(t *testing.T) {
	fixture := mustLocateFixtures(t)

	summary, err := ReadDoneSummary(fixture.done)
	if err != nil {
		t.Fatalf("ReadDoneSummary failed: %v", err)
	}
	if summary.Total != 1 {
		t.Fatalf("expected 1 done item, got %d", summary.Total)
	}
	if summary.LastID != "RQ-0005" {
		t.Fatalf("expected last done ID RQ-0005, got %q", summary.LastID)
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

func TestMoveCheckedToDonePrependUpdatesDoneSummary(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	donePath := filepath.Join(tmpDir, "implementation_done.md")

	queueContent := strings.Join([]string{
		"# Implementation Queue",
		"",
		"## Queue",
		"- [x] RQ-0101 [code]: Move to done. (x)",
		"  - Evidence: done",
		"  - Plan: done",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	doneContent := strings.Join([]string{
		"# Implementation Done",
		"",
		"## Done",
		"- [x] RQ-0001 [code]: Older done. (x)",
		"  - Evidence: older",
		"  - Plan: done",
		"",
	}, "\n")

	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}
	if err := os.WriteFile(donePath, []byte(doneContent), 0o600); err != nil {
		t.Fatalf("write done: %v", err)
	}

	ids, err := MoveCheckedToDone(queuePath, donePath, true)
	if err != nil {
		t.Fatalf("MoveCheckedToDone failed: %v", err)
	}
	if len(ids) != 1 || ids[0] != "RQ-0101" {
		t.Fatalf("expected moved ID RQ-0101, got %#v", ids)
	}

	summary, err := ReadDoneSummary(donePath)
	if err != nil {
		t.Fatalf("ReadDoneSummary failed: %v", err)
	}
	if summary.Total != 2 {
		t.Fatalf("expected 2 done items, got %d", summary.Total)
	}
	if summary.LastID != "RQ-0101" {
		t.Fatalf("expected last done ID RQ-0101, got %q", summary.LastID)
	}
}

func TestMoveCheckedToDoneMovesUppercaseX(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	donePath := filepath.Join(tmpDir, "implementation_done.md")

	queueContent := strings.Join([]string{
		"## Queue",
		"- [X] RQ-0100 [code]: Move uppercase checkbox. (x)",
		"  - Evidence: uppercase checkbox should move.",
		"  - Plan: verify MoveCheckedToDone handles [X].",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	if err := os.WriteFile(queuePath, []byte(queueContent), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}
	if err := os.WriteFile(donePath, []byte("## Done\n"), 0o600); err != nil {
		t.Fatalf("write done: %v", err)
	}

	ids, err := MoveCheckedToDone(queuePath, donePath, true)
	if err != nil {
		t.Fatalf("MoveCheckedToDone failed: %v", err)
	}
	if len(ids) != 1 || ids[0] != "RQ-0100" {
		t.Fatalf("expected moved ID RQ-0100, got %#v", ids)
	}

	queueLines := readFileLines(t, queuePath)
	for _, line := range queueLines {
		if strings.Contains(line, "RQ-0100") {
			t.Fatalf("expected queue item removed, found %q", line)
		}
	}

	doneLines := readFileLines(t, donePath)
	found := false
	for _, line := range doneLines {
		if strings.Contains(line, "RQ-0100") {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected done item to be inserted")
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

func TestToggleQueueItemCheckedHandlesUppercaseX(t *testing.T) {
	tmpDir := t.TempDir()
	queuePath := filepath.Join(tmpDir, "implementation_queue.md")
	content := strings.Join([]string{
		"## Queue",
		"- [X] RQ-0200 [code]: Toggle uppercase checkbox. (x)",
		"  - Evidence: uppercase checkbox should toggle.",
		"  - Plan: ensure toggle normalizes to unchecked.",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	if err := os.WriteFile(queuePath, []byte(content), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	ok, checked, err := ToggleQueueItemChecked(queuePath, "RQ-0200")
	if err != nil {
		t.Fatalf("ToggleQueueItemChecked failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected toggle to succeed")
	}
	if checked {
		t.Fatalf("expected item to be unchecked after toggle")
	}

	lines := readFileLines(t, queuePath)
	if len(lines) < 2 || !strings.HasPrefix(lines[1], "- [ ]") {
		if len(lines) < 2 {
			t.Fatalf("expected queue header to exist after toggle")
		}
		t.Fatalf("expected header to be unchecked, got %q", lines[1])
	}
}

func TestRequeueBlockedItem(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	ok, _, err := RecordFixupAttempt(queuePath, "RQ-0003", "2026-01-15T00:00:00Z ci failed")
	if err != nil {
		t.Fatalf("RecordFixupAttempt failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected fixup attempt to update blocked item")
	}

	ok, err = RequeueBlockedItem(queuePath, "RQ-0003", RequeueOptions{InsertAtTop: true})
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
			strings.HasPrefix(trimmed, "- Unblock hint:") ||
			strings.HasPrefix(trimmed, "- Fixup attempts:") ||
			strings.HasPrefix(trimmed, "- Fixup last:") {
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

func TestResetFixupMetadata(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	ok, _, err := RecordFixupAttempt(queuePath, "RQ-0003", "2026-01-15T00:00:00Z ci failed")
	if err != nil {
		t.Fatalf("RecordFixupAttempt failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected fixup attempt to update blocked item")
	}

	found, changed, err := ResetFixupMetadata(queuePath, "RQ-0003")
	if err != nil {
		t.Fatalf("ResetFixupMetadata failed: %v", err)
	}
	if !found {
		t.Fatalf("expected blocked item to be found")
	}
	if !changed {
		t.Fatalf("expected fixup metadata to be removed")
	}

	lines := readFileLines(t, queuePath)
	block, section := findItemBlock(lines, "- [ ] RQ-0003 [ops]: Blocked fixture item. (README.md)")
	if section != "Blocked" {
		t.Fatalf("expected item in Blocked section, got %s", section)
	}
	for _, line := range block {
		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "- Fixup attempts:") || strings.HasPrefix(trimmed, "- Fixup last:") {
			t.Fatalf("expected fixup metadata removed, found %q", trimmed)
		}
	}

	items, err := ReadBlockedItems(queuePath)
	if err != nil {
		t.Fatalf("ReadBlockedItems failed: %v", err)
	}
	if len(items) == 0 {
		t.Fatalf("expected blocked items to remain")
	}
	if items[0].FixupAttempts != 0 {
		t.Fatalf("expected fixup attempts reset to 0, got %d", items[0].FixupAttempts)
	}
	if items[0].FixupLast != "" {
		t.Fatalf("expected fixup last reset, got %q", items[0].FixupLast)
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
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	donePath := copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

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

	if err := ValidatePin(files, project.TypeCode); err != nil {
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
	specsCode := filepath.Join(pinDir, SpecsBuilderCodeFilename)
	specsDocs := filepath.Join(pinDir, SpecsBuilderDocsFilename)
	return fixturePaths{
		pinDir:    pinDir,
		queue:     queue,
		done:      done,
		lookup:    lookup,
		readme:    readme,
		specsCode: specsCode,
		specsDocs: specsDocs,
	}
}

func mustLocateEmptyFixtures(t *testing.T) fixturePaths {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("unable to resolve testdata path")
	}

	baseDir := filepath.Dir(file)
	pinDir := filepath.Join(baseDir, "testdata", "pin_empty")
	queue := filepath.Join(pinDir, "implementation_queue.md")
	if !fileExists(queue) {
		t.Fatalf("unable to locate empty pin fixtures from %s", pinDir)
	}

	done := filepath.Join(pinDir, "implementation_done.md")
	lookup := filepath.Join(pinDir, "lookup_table.md")
	readme := filepath.Join(pinDir, "README.md")
	specsCode := filepath.Join(pinDir, SpecsBuilderCodeFilename)
	specsDocs := filepath.Join(pinDir, SpecsBuilderDocsFilename)
	return fixturePaths{
		pinDir:    pinDir,
		queue:     queue,
		done:      done,
		lookup:    lookup,
		readme:    readme,
		specsCode: specsCode,
		specsDocs: specsDocs,
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

func copyFixtureSpecs(t *testing.T, fixture fixturePaths, files Files) {
	t.Helper()
	_ = copyFixture(t, fixture.specsCode, files.SpecsBuilderCodePath)
	_ = copyFixture(t, fixture.specsDocs, files.SpecsBuilderDocsPath)
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
