// Package pin provides validation and deterministic operations for Ralph pin files.
// Entrypoint: ValidatePin, MoveCheckedToDone, BlockItem.
package pin

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/queueid"
)

var (
	tagPattern    = regexp.MustCompile(`\[(db|ui|code|ops|docs)\]`)
	scopePattern  = regexp.MustCompile(`\([^()]+\)\s*$`)
	queueItemLine = regexp.MustCompile(`^- \[[ x]\] `)
)

// Files describes the Ralph pin/spec files on disk.
type Files struct {
	QueuePath  string
	DonePath   string
	LookupPath string
	ReadmePath string
}

// ResolveFiles returns the expected pin file locations for the given repo.
func ResolveFiles(pinDir string) Files {
	return Files{
		QueuePath:  filepath.Join(pinDir, "implementation_queue.md"),
		DonePath:   filepath.Join(pinDir, "implementation_done.md"),
		LookupPath: filepath.Join(pinDir, "lookup_table.md"),
		ReadmePath: filepath.Join(pinDir, "README.md"),
	}
}

// ValidatePin enforces the pin/spec validation rules.
func ValidatePin(files Files) error {
	if err := requireFile(files.QueuePath); err != nil {
		return err
	}
	if err := requireFile(files.DonePath); err != nil {
		return err
	}
	if err := requireFile(files.LookupPath); err != nil {
		return err
	}
	if err := requireFile(files.ReadmePath); err != nil {
		return err
	}

	queueLines, err := readLines(files.QueuePath)
	if err != nil {
		return err
	}
	doneLines, err := readLines(files.DonePath)
	if err != nil {
		return err
	}

	if err := ensureQueueSections(queueLines); err != nil {
		return err
	}

	ids := append(extractIDs(queueLines), extractIDs(doneLines)...)
	sort.Strings(ids)
	if len(ids) == 0 {
		return fmt.Errorf("No task IDs found in queue/done. Expected IDs like RQ-0123.")
	}
	dupes := findDuplicates(ids)
	if len(dupes) > 0 {
		return fmt.Errorf("Duplicate task IDs detected. Fix these IDs:\n%s", strings.Join(dupes, "\n"))
	}

	missingIDs := missingIDLines(queueLines)
	if len(missingIDs) > 0 {
		return fmt.Errorf("Queue has top-level items missing an ID:\n%s", strings.Join(missingIDs, "\n"))
	}

	if err := validateQueueItemFormat(queueLines); err != nil {
		return err
	}

	return nil
}

// QueueItem represents a queue block in the Queue section.
type QueueItem struct {
	Header  string
	Lines   []string
	ID      string
	Checked bool
}

// ReadQueueItems returns queue items from the Queue section.
func ReadQueueItems(queuePath string) ([]QueueItem, error) {
	lines, err := readLines(queuePath)
	if err != nil {
		return nil, err
	}

	blocks := splitBlocks(lines)
	items := make([]QueueItem, 0)
	inQueue := false
	for _, block := range blocks {
		if len(block) == 0 {
			continue
		}
		header := block[0]
		if strings.TrimSpace(header) == "## Queue" {
			inQueue = true
			continue
		}
		if strings.HasPrefix(header, "## ") {
			inQueue = false
			continue
		}
		if !inQueue {
			continue
		}
		if strings.HasPrefix(header, "- [") {
			items = append(items, QueueItem{
				Header:  header,
				Lines:   block,
				ID:      extractID(header),
				Checked: strings.HasPrefix(header, "- [x]"),
			})
		}
	}

	return items, nil
}

// MoveCheckedToDone moves checked blocks from Queue to Done.
func MoveCheckedToDone(queuePath string, donePath string, prepend bool) ([]string, error) {
	if err := requireFile(queuePath); err != nil {
		return nil, err
	}
	if err := requireFile(donePath); err != nil {
		return nil, err
	}

	queueLines, err := readLines(queuePath)
	if err != nil {
		return nil, err
	}
	doneLines, err := readLines(donePath)
	if err != nil {
		return nil, err
	}

	blocks := splitBlocks(queueLines)
	newQueue := make([]string, 0)
	moved := make([][]string, 0)
	inQueue := false
	ids := make([]string, 0)

	for _, block := range blocks {
		header := firstLine(block)
		if strings.TrimSpace(header) == "## Queue" {
			inQueue = true
			newQueue = append(newQueue, block...)
			continue
		}
		if strings.HasPrefix(header, "## ") {
			inQueue = false
			newQueue = append(newQueue, block...)
			continue
		}
		if inQueue && strings.HasPrefix(header, "- [x]") {
			moved = append(moved, block)
			if match := queueid.Extract(header); match != "" {
				ids = append(ids, match)
			}
			continue
		}
		newQueue = append(newQueue, block...)
	}

	if len(moved) > 0 {
		doneIndex, updated := ensureDoneSection(doneLines)
		doneLines = updated
		insertPos := doneIndex + 1

		inserted := make([]string, 0)
		for _, block := range moved {
			inserted = append(inserted, block...)
		}

		if prepend {
			doneLines = insertLines(doneLines, insertPos, inserted)
		} else {
			sectionEnd := len(doneLines)
			for i := doneIndex + 1; i < len(doneLines); i++ {
				if strings.HasPrefix(doneLines[i], "## ") {
					sectionEnd = i
					break
				}
			}
			doneLines = insertLines(doneLines, sectionEnd, inserted)
		}

		if err := writeLines(donePath, doneLines); err != nil {
			return nil, err
		}
	}

	if err := writeLines(queuePath, newQueue); err != nil {
		return nil, err
	}

	return uniqueIDs(ids), nil
}

// Metadata captures optional fields for blocking an item.
type Metadata struct {
	WIPBranch   string
	KnownGood   string
	UnblockHint string
}

// BlockItem moves a queue item into Blocked and appends metadata.
func BlockItem(queuePath string, itemID string, reasonLines []string, metadata Metadata) (bool, error) {
	if err := requireFile(queuePath); err != nil {
		return false, err
	}

	lines, err := readLines(queuePath)
	if err != nil {
		return false, err
	}
	blocks := splitBlocks(lines)
	newBlocks := make([][]string, 0)

	inQueue := false
	queueIndex := -1
	blockedIndex := -1
	var itemBlock []string

	for _, block := range blocks {
		header := firstLine(block)
		if strings.TrimSpace(header) == "## Queue" {
			inQueue = true
			queueIndex = len(newBlocks)
			newBlocks = append(newBlocks, block)
			continue
		}
		if strings.TrimSpace(header) == "## Blocked" {
			inQueue = false
			blockedIndex = len(newBlocks)
			newBlocks = append(newBlocks, block)
			continue
		}
		if strings.HasPrefix(header, "## ") {
			inQueue = false
			newBlocks = append(newBlocks, block)
			continue
		}
		if inQueue && strings.HasPrefix(header, "- [") && extractID(header) == itemID {
			itemBlock = block
			continue
		}
		newBlocks = append(newBlocks, block)
	}

	if itemBlock == nil {
		return false, nil
	}

	itemBlock = appendMetadata(itemBlock, reasonLines, metadata)

	if blockedIndex >= 0 {
		insertPos := blockedIndex + 1
		newBlocks = insertBlocks(newBlocks, insertPos, itemBlock)
	} else {
		if queueIndex < 0 {
			return false, fmt.Errorf("Queue section not found while blocking item.")
		}
		insertPos := findSectionEnd(newBlocks, queueIndex)
		newBlocks = insertBlocks(newBlocks, insertPos, []string{"## Blocked"})
		newBlocks = insertBlocks(newBlocks, insertPos+1, itemBlock)
	}

	flattened := flattenBlocks(newBlocks)
	if err := writeLines(queuePath, flattened); err != nil {
		return false, err
	}

	return true, nil
}

// ToggleQueueItemChecked flips the checked state for a queue item by ID.
func ToggleQueueItemChecked(queuePath string, itemID string) (bool, bool, error) {
	if err := requireFile(queuePath); err != nil {
		return false, false, err
	}

	lines, err := readLines(queuePath)
	if err != nil {
		return false, false, err
	}

	blocks := splitBlocks(lines)
	newBlocks := make([][]string, 0, len(blocks))
	updated := false
	checked := false
	inQueue := false

	for _, block := range blocks {
		header := firstLine(block)
		switch {
		case strings.TrimSpace(header) == "## Queue":
			inQueue = true
			newBlocks = append(newBlocks, block)
			continue
		case strings.HasPrefix(header, "## "):
			inQueue = false
			newBlocks = append(newBlocks, block)
			continue
		}

		if inQueue && strings.HasPrefix(header, "- [") && extractID(header) == itemID {
			header = toggleCheckHeader(header)
			block[0] = header
			updated = true
			checked = strings.HasPrefix(header, "- [x]")
		}
		newBlocks = append(newBlocks, block)
	}

	if !updated {
		return false, false, nil
	}

	flattened := flattenBlocks(newBlocks)
	if err := writeLines(queuePath, flattened); err != nil {
		return false, false, err
	}

	return true, checked, nil
}

func toggleCheckHeader(header string) string {
	trimmed := strings.TrimLeft(header, " \t")
	if strings.HasPrefix(trimmed, "- [x]") {
		return strings.Replace(header, "- [x]", "- [ ]", 1)
	}
	if strings.HasPrefix(trimmed, "- [ ]") {
		return strings.Replace(header, "- [ ]", "- [x]", 1)
	}
	return header
}

func appendMetadata(block []string, reasonLines []string, metadata Metadata) []string {
	indent := "  "
	for _, line := range reasonLines {
		clean := strings.TrimSpace(line)
		if clean != "" {
			block = append(block, fmt.Sprintf("%s- Blocked reason: %s", indent, clean))
		}
	}
	if metadata.WIPBranch != "" {
		block = append(block, fmt.Sprintf("%s- WIP branch: %s", indent, metadata.WIPBranch))
	}
	if metadata.KnownGood != "" {
		block = append(block, fmt.Sprintf("%s- Known-good: %s", indent, metadata.KnownGood))
	}
	if metadata.UnblockHint != "" {
		block = append(block, fmt.Sprintf("%s- Unblock hint: %s", indent, metadata.UnblockHint))
	}
	return block
}

func ensureQueueSections(lines []string) error {
	queueFound := false
	blockedFound := false
	parkingFound := false

	for _, line := range lines {
		if strings.TrimSpace(line) == "## Queue" {
			queueFound = true
		}
		if strings.TrimSpace(line) == "## Blocked" {
			blockedFound = true
		}
		if strings.TrimSpace(line) == "## Parking Lot" {
			parkingFound = true
		}
	}

	if !queueFound {
		return fmt.Errorf("Queue file missing '## Queue'")
	}
	if !blockedFound {
		return fmt.Errorf("Queue file missing '## Blocked'")
	}
	if !parkingFound {
		return fmt.Errorf("Queue file missing '## Parking Lot'")
	}
	return nil
}

func extractIDs(lines []string) []string {
	ids := make([]string, 0)
	for _, line := range lines {
		if queueItemLine.MatchString(line) {
			if match := queueid.Extract(line); match != "" {
				ids = append(ids, match)
			}
		}
	}
	return ids
}

func extractID(line string) string {
	return queueid.Extract(line)
}

func missingIDLines(lines []string) []string {
	missing := make([]string, 0)
	for _, line := range lines {
		if queueItemLine.MatchString(line) && queueid.Extract(line) == "" {
			missing = append(missing, line)
		}
	}
	return missing
}

func validateQueueItemFormat(lines []string) error {
	inQueue := false
	itemActive := false
	header := ""
	itemLines := make([]string, 0)
	bad := false
	output := make([]string, 0)

	startItem := func(line string) {
		header = line
		itemLines = itemLines[:0]
		itemActive = true
	}

	finishItem := func() {
		if !itemActive {
			return
		}

		idOk := queueid.Extract(header) != ""
		tagOk := tagPattern.MatchString(header)
		colonOk := strings.Contains(header, ": ")
		scopeOk := scopePattern.MatchString(header)

		evidenceOk := false
		planOk := false
		for _, line := range itemLines {
			trimmed := strings.TrimLeft(line, " \t")
			if strings.HasPrefix(trimmed, "- Evidence:") {
				evidenceOk = true
			}
			if strings.HasPrefix(trimmed, "- Plan:") {
				planOk = true
			}
		}

		if !(idOk && tagOk && colonOk && scopeOk && evidenceOk && planOk) {
			bad = true
			output = append(output, "Bad queue item format:")
			output = append(output, header)
			if !idOk {
				output = append(output, "  - Missing ID like RQ-0123")
			}
			if !tagOk {
				output = append(output, "  - Missing routing tag like [code]/[db]/[ui]/[ops]/[docs]")
			}
			if !colonOk {
				output = append(output, "  - Missing \":\" after ID/tags")
			}
			if !scopeOk {
				output = append(output, "  - Missing trailing scope list in parentheses, e.g. (path/to/file.py, Makefile)")
			}
			if !evidenceOk {
				output = append(output, "  - Missing indented metadata bullet: \"- Evidence: ...\"")
			}
			if !planOk {
				output = append(output, "  - Missing indented metadata bullet: \"- Plan: ...\"")
			}
			output = append(output, "")
		}

		header = ""
		itemLines = itemLines[:0]
		itemActive = false
	}

	for _, line := range lines {
		switch {
		case strings.TrimSpace(line) == "## Queue":
			finishItem()
			inQueue = true
		case strings.HasPrefix(line, "## "):
			if inQueue {
				finishItem()
			}
			inQueue = false
		case inQueue:
			if queueItemLine.MatchString(line) {
				finishItem()
				startItem(line)
				continue
			}
			if itemActive {
				itemLines = append(itemLines, line)
			}
		}
	}

	finishItem()

	if bad {
		return fmt.Errorf(
			"Queue items in ## Queue must follow the required format (ID + routing tag(s) + scope + Evidence + Plan).\n\n%s",
			strings.Join(output, "\n"),
		)
	}
	return nil
}

func splitBlocks(lines []string) [][]string {
	blocks := make([][]string, 0)
	current := make([]string, 0)

	for _, line := range lines {
		if strings.HasPrefix(line, "- [") || strings.HasPrefix(line, "## ") {
			if len(current) > 0 {
				blocks = append(blocks, current)
			}
			current = []string{line}
			continue
		}
		if len(current) > 0 {
			current = append(current, line)
		} else {
			blocks = append(blocks, []string{line})
			current = []string{}
		}
	}
	if len(current) > 0 {
		blocks = append(blocks, current)
	}
	return blocks
}

func summarizeIDs(ids []string) string {
	if len(ids) == 0 {
		return ""
	}
	unique := uniqueIDs(ids)
	if len(unique) == 1 {
		return unique[0]
	}
	if len(unique) == 2 {
		return fmt.Sprintf("%s, %s", unique[0], unique[1])
	}
	return fmt.Sprintf("%s +%d", unique[0], len(unique)-1)
}

func ensureDoneSection(doneLines []string) (int, []string) {
	for idx, line := range doneLines {
		if line == "## Done" {
			return idx, doneLines
		}
	}
	insertAt := 0
	if len(doneLines) > 0 && strings.HasPrefix(doneLines[0], "#") {
		insertAt = 1
	}
	doneLines = insertLines(doneLines, insertAt, []string{"## Done"})
	return insertAt, doneLines
}

func findSectionEnd(blocks [][]string, headerIndex int) int {
	idx := headerIndex + 1
	for idx < len(blocks) {
		header := firstLine(blocks[idx])
		if strings.HasPrefix(header, "## ") {
			break
		}
		idx++
	}
	return idx
}

func insertLines(lines []string, index int, insert []string) []string {
	if index < 0 || index > len(lines) {
		return append(lines, insert...)
	}
	result := make([]string, 0, len(lines)+len(insert))
	result = append(result, lines[:index]...)
	result = append(result, insert...)
	result = append(result, lines[index:]...)
	return result
}

func insertBlocks(blocks [][]string, index int, block []string) [][]string {
	if index < 0 || index > len(blocks) {
		return append(blocks, block)
	}
	result := make([][]string, 0, len(blocks)+1)
	result = append(result, blocks[:index]...)
	result = append(result, block)
	result = append(result, blocks[index:]...)
	return result
}

func flattenBlocks(blocks [][]string) []string {
	flattened := make([]string, 0)
	for _, block := range blocks {
		flattened = append(flattened, block...)
	}
	return flattened
}

func uniqueIDs(ids []string) []string {
	seen := make(map[string]bool)
	unique := make([]string, 0)
	for _, id := range ids {
		if !seen[id] {
			seen[id] = true
			unique = append(unique, id)
		}
	}
	return unique
}

func findDuplicates(ids []string) []string {
	counts := make(map[string]int)
	for _, id := range ids {
		counts[id]++
	}
	dupes := make([]string, 0)
	for _, id := range ids {
		if counts[id] > 1 && !contains(dupes, id) {
			dupes = append(dupes, id)
		}
	}
	return dupes
}

func contains(slice []string, value string) bool {
	for _, item := range slice {
		if item == value {
			return true
		}
	}
	return false
}

func firstLine(block []string) string {
	if len(block) == 0 {
		return ""
	}
	return block[0]
}

func readLines(path string) ([]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	content := strings.TrimSuffix(string(data), "\n")
	if content == "" {
		return []string{}, nil
	}
	return strings.Split(content, "\n"), nil
}

func writeLines(path string, lines []string) error {
	payload := strings.Join(lines, "\n") + "\n"
	return os.WriteFile(path, []byte(payload), 0o600)
}

func requireFile(path string) error {
	info, err := os.Stat(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("Missing %s", path)
		}
		return err
	}
	if info.IsDir() {
		return fmt.Errorf("Missing %s", path)
	}
	return nil
}

// SummarizeIDs returns the human-friendly summary for moved IDs.
func SummarizeIDs(ids []string) string {
	return summarizeIDs(ids)
}
