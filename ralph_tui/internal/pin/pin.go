// Package pin provides validation and deterministic operations for Ralph pin files.
// Entrypoint: ValidatePin, MoveCheckedToDone, ReadQueueItems, ReadDoneItems, BlockItem.
package pin

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/fileutil"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/queueid"
)

var (
	tagPattern      = regexp.MustCompile(`(?i)\[(db|ui|code|ops|docs)\]`)
	scopePattern    = regexp.MustCompile(`\([^()]+\)\s*$`)
	queueItemLine   = regexp.MustCompile(`^- \[[ xX]\] `)
	supportedTags   = []string{"db", "ui", "code", "ops", "docs"}
	supportedTagSet = map[string]struct{}{
		"db":   {},
		"ui":   {},
		"code": {},
		"ops":  {},
		"docs": {},
	}
)

const (
	metadataIndent      = "  "
	blockedReasonPrefix = "- Blocked reason:"
	wipBranchPrefix     = "- WIP branch:"
	knownGoodPrefix     = "- Known-good:"
	unblockHintPrefix   = "- Unblock hint:"
	fixupAttemptsPrefix = "- Fixup attempts:"
	fixupLastPrefix     = "- Fixup last:"
)

const (
	SpecsBuilderCodeFilename = "specs_builder.md"
	SpecsBuilderDocsFilename = "specs_builder_docs.md"
)

// TagList captures parsed tags plus any unknown values.
type TagList struct {
	Tags    []string
	Unknown []string
}

// SupportedTags returns the supported routing tags in display order.
func SupportedTags() []string {
	return append([]string{}, supportedTags...)
}

// NormalizeTag trims, unbrackets, and lowercases a tag value.
func NormalizeTag(value string) string {
	trimmed := strings.TrimSpace(value)
	if trimmed == "" {
		return ""
	}
	trimmed = strings.TrimPrefix(trimmed, "[")
	trimmed = strings.TrimSuffix(trimmed, "]")
	return strings.ToLower(strings.TrimSpace(trimmed))
}

// ExtractTags returns the normalized routing tags from a queue item header.
func ExtractTags(header string) []string {
	search := header
	if idx := strings.Index(header, ":"); idx != -1 {
		search = header[:idx]
	}
	matches := tagPattern.FindAllStringSubmatch(search, -1)
	if len(matches) == 0 {
		return nil
	}
	tags := make([]string, 0, len(matches))
	seen := make(map[string]struct{}, len(matches))
	for _, match := range matches {
		if len(match) < 2 {
			continue
		}
		tag := NormalizeTag(match[1])
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		tags = append(tags, tag)
	}
	return tags
}

// ParseTagList parses a tag list from a comma/space separated string.
func ParseTagList(input string) TagList {
	if strings.TrimSpace(input) == "" {
		return TagList{Tags: []string{}, Unknown: []string{}}
	}
	normalized := strings.ReplaceAll(input, ",", " ")
	fields := strings.Fields(normalized)
	tags := make([]string, 0, len(fields))
	unknown := make([]string, 0)
	seen := make(map[string]struct{}, len(fields))
	unknownSeen := make(map[string]struct{}, len(fields))
	for _, field := range fields {
		tag := NormalizeTag(field)
		if tag == "" {
			continue
		}
		if _, ok := supportedTagSet[tag]; !ok {
			if _, dup := unknownSeen[tag]; !dup {
				unknownSeen[tag] = struct{}{}
				unknown = append(unknown, tag)
			}
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		tags = append(tags, tag)
	}
	return TagList{Tags: tags, Unknown: unknown}
}

// ValidateTagList parses tags and returns an error that names the source when unsupported tags are present.
func ValidateTagList(source string, input string) ([]string, error) {
	parsed := ParseTagList(input)
	if len(parsed.Unknown) > 0 {
		return nil, fmt.Errorf(
			"%s has unsupported tag(s): %s (supported: %s)",
			source,
			strings.Join(parsed.Unknown, ", "),
			strings.Join(SupportedTags(), ", "),
		)
	}
	return parsed.Tags, nil
}

// MatchesAnyTag returns true if the header contains any of the provided tags.
func MatchesAnyTag(header string, tags []string) bool {
	if len(tags) == 0 {
		return true
	}
	headerTags := ExtractTags(header)
	if len(headerTags) == 0 {
		return false
	}
	tagSet := make(map[string]struct{}, len(headerTags))
	for _, tag := range headerTags {
		tagSet[tag] = struct{}{}
	}
	for _, tag := range tags {
		normalized := NormalizeTag(tag)
		if normalized == "" {
			continue
		}
		if _, ok := tagSet[normalized]; ok {
			return true
		}
	}
	return false
}

// Files describes the Ralph pin/spec files on disk.
type Files struct {
	QueuePath            string
	DonePath             string
	LookupPath           string
	ReadmePath           string
	SpecsBuilderCodePath string
	SpecsBuilderDocsPath string
}

// ResolveFiles returns the expected pin file locations for the given repo.
func ResolveFiles(pinDir string) Files {
	return Files{
		QueuePath:            filepath.Join(pinDir, "implementation_queue.md"),
		DonePath:             filepath.Join(pinDir, "implementation_done.md"),
		LookupPath:           filepath.Join(pinDir, "lookup_table.md"),
		ReadmePath:           filepath.Join(pinDir, "README.md"),
		SpecsBuilderCodePath: filepath.Join(pinDir, SpecsBuilderCodeFilename),
		SpecsBuilderDocsPath: filepath.Join(pinDir, SpecsBuilderDocsFilename),
	}
}

// AllPaths returns the full paths of all pin files.
func (f Files) AllPaths() []string {
	return []string{
		f.QueuePath,
		f.DonePath,
		f.LookupPath,
		f.ReadmePath,
		f.SpecsBuilderCodePath,
		f.SpecsBuilderDocsPath,
	}
}

// RequiredSpecsBuilderPath returns the specs builder template for the given project type.
func (f Files) RequiredSpecsBuilderPath(projectType project.Type) (string, error) {
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", fmt.Errorf("project_type must be code or docs")
	}
	if resolvedType == project.TypeDocs {
		return f.SpecsBuilderDocsPath, nil
	}
	return f.SpecsBuilderCodePath, nil
}

// RequiredPaths returns pin files required for validation.
func (f Files) RequiredPaths(projectType project.Type) ([]string, error) {
	if _, err := project.ResolveType(projectType); err != nil {
		return nil, fmt.Errorf("project_type must be code or docs")
	}
	return []string{
		f.QueuePath,
		f.DonePath,
		f.LookupPath,
		f.ReadmePath,
		f.SpecsBuilderCodePath,
		f.SpecsBuilderDocsPath,
	}, nil
}

// RelativePaths returns repo-relative paths for all pin files, using forward slashes.
func (f Files) RelativePaths(repoRoot string) []string {
	paths := f.AllPaths()
	relatives := make([]string, 0, len(paths))
	for _, path := range paths {
		rel, err := filepath.Rel(repoRoot, path)
		if err != nil {
			continue
		}
		relatives = append(relatives, filepath.ToSlash(rel))
	}
	return relatives
}

// RelativePathSet returns a set of repo-relative pin file paths.
func (f Files) RelativePathSet(repoRoot string) map[string]struct{} {
	relatives := f.RelativePaths(repoRoot)
	set := make(map[string]struct{}, len(relatives))
	for _, path := range relatives {
		set[path] = struct{}{}
	}
	return set
}

// ValidatePin enforces the pin/spec validation rules.
func ValidatePin(files Files, projectType project.Type) error {
	required, err := files.RequiredPaths(projectType)
	if err != nil {
		return err
	}
	for _, path := range required {
		if err := requireFile(path); err != nil {
			return err
		}
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

// DoneSummary captures the Done section overview.
type DoneSummary struct {
	LastID string
	Total  int
}

// BlockedItem represents a queue block in the Blocked section.
type BlockedItem struct {
	Header        string
	Lines         []string
	ID            string
	Metadata      Metadata
	FixupAttempts int
	FixupLast     string
}

// ReadDoneSummary returns the Done section summary.
//
// LastID is the most recent done item, which is expected to be at the top
// of the Done section (prepend semantics). Ralph defaults to prepending new
// Done items in the CLI, TUI, and loop runner.
func ReadDoneSummary(donePath string) (DoneSummary, error) {
	lines, err := readLines(donePath)
	if err != nil {
		return DoneSummary{}, err
	}

	blocks := splitBlocks(lines)
	summary := DoneSummary{}
	inDone := false
	for _, block := range blocks {
		if len(block) == 0 {
			continue
		}
		header := block[0]
		switch {
		case strings.TrimSpace(header) == "## Done":
			inDone = true
			continue
		case strings.HasPrefix(header, "## "):
			if inDone {
				inDone = false
			}
			continue
		}

		if !inDone {
			continue
		}
		if !strings.HasPrefix(header, "- [") {
			continue
		}
		summary.Total++
		if summary.LastID == "" {
			summary.LastID = extractID(header)
		}
	}

	return summary, nil
}

// ReadDoneItems returns done items from the Done section.
func ReadDoneItems(donePath string) ([]QueueItem, error) {
	lines, err := readLines(donePath)
	if err != nil {
		return nil, err
	}

	blocks := splitBlocks(lines)
	items := make([]QueueItem, 0)
	inDone := false
	for _, block := range blocks {
		if len(block) == 0 {
			continue
		}
		header := block[0]
		switch {
		case strings.TrimSpace(header) == "## Done":
			inDone = true
			continue
		case strings.HasPrefix(header, "## "):
			inDone = false
			continue
		}

		if !inDone || !strings.HasPrefix(header, "- [") {
			continue
		}
		items = append(items, QueueItem{
			Header:  header,
			Lines:   block,
			ID:      extractID(header),
			Checked: isHeaderChecked(header),
		})
	}

	return items, nil
}

// ReadQueueSummary returns queue items plus the blocked item count.
func ReadQueueSummary(queuePath string) ([]QueueItem, int, error) {
	lines, err := readLines(queuePath)
	if err != nil {
		return nil, 0, err
	}

	blocks := splitBlocks(lines)
	items := make([]QueueItem, 0)
	blockedCount := 0
	inQueue := false
	inBlocked := false
	for _, block := range blocks {
		if len(block) == 0 {
			continue
		}
		header := block[0]
		switch {
		case strings.TrimSpace(header) == "## Queue":
			inQueue = true
			inBlocked = false
			continue
		case strings.TrimSpace(header) == "## Blocked":
			inQueue = false
			inBlocked = true
			continue
		case strings.HasPrefix(header, "## "):
			inQueue = false
			inBlocked = false
			continue
		}

		if !strings.HasPrefix(header, "- [") {
			continue
		}
		if inQueue {
			items = append(items, QueueItem{
				Header:  header,
				Lines:   block,
				ID:      extractID(header),
				Checked: isHeaderChecked(header),
			})
			continue
		}
		if inBlocked {
			blockedCount++
		}
	}

	return items, blockedCount, nil
}

// ReadBlockedItems returns blocked items from the Blocked section with metadata.
func ReadBlockedItems(queuePath string) ([]BlockedItem, error) {
	lines, err := readLines(queuePath)
	if err != nil {
		return nil, err
	}

	blocks := splitBlocks(lines)
	items := make([]BlockedItem, 0)
	inBlocked := false

	for _, block := range blocks {
		if len(block) == 0 {
			continue
		}
		header := block[0]
		switch {
		case strings.TrimSpace(header) == "## Blocked":
			inBlocked = true
			continue
		case strings.HasPrefix(header, "## "):
			inBlocked = false
			continue
		}

		if inBlocked && strings.HasPrefix(header, "- [") {
			items = append(items, parseBlockedItem(block))
		}
	}

	return items, nil
}

// ReadQueueItems returns queue items from the Queue section.
func ReadQueueItems(queuePath string) ([]QueueItem, error) {
	items, _, err := ReadQueueSummary(queuePath)
	return items, err
}

// MoveCheckedToDone moves checked blocks from Queue to Done.
func MoveCheckedToDone(queuePath string, donePath string, prepend bool) ([]string, error) {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return nil, err
	}
	defer lock.Release()

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
		if inQueue && isHeaderChecked(header) {
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
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return false, err
	}
	defer lock.Release()

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

// RequeueBlockedItem moves a Blocked item back to the Queue section.
func RequeueBlockedItem(queuePath string, itemID string, opts RequeueOptions) (bool, error) {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return false, err
	}
	defer lock.Release()

	if err := requireFile(queuePath); err != nil {
		return false, err
	}

	lines, err := readLines(queuePath)
	if err != nil {
		return false, err
	}

	blocks := splitBlocks(lines)
	newBlocks := make([][]string, 0, len(blocks))
	inBlocked := false
	queueIndex := -1
	var itemBlock []string

	for _, block := range blocks {
		header := firstLine(block)
		switch {
		case strings.TrimSpace(header) == "## Queue":
			inBlocked = false
			queueIndex = len(newBlocks)
			newBlocks = append(newBlocks, block)
			continue
		case strings.TrimSpace(header) == "## Blocked":
			inBlocked = true
			newBlocks = append(newBlocks, block)
			continue
		case strings.HasPrefix(header, "## "):
			inBlocked = false
			newBlocks = append(newBlocks, block)
			continue
		}

		if inBlocked && strings.HasPrefix(header, "- [") && extractID(header) == itemID {
			itemBlock = block
			continue
		}
		newBlocks = append(newBlocks, block)
	}

	if itemBlock == nil {
		return false, nil
	}
	if queueIndex < 0 {
		return false, fmt.Errorf("Queue section not found while requeueing item.")
	}

	itemBlock = stripBlockedMetadata(itemBlock)
	itemBlock[0] = setHeaderUnchecked(itemBlock[0])

	insertPos := queueIndex + 1
	if !opts.InsertAtTop {
		insertPos = findSectionEnd(newBlocks, queueIndex)
	}
	newBlocks = insertBlocks(newBlocks, insertPos, itemBlock)

	flattened := flattenBlocks(newBlocks)
	if err := writeLines(queuePath, flattened); err != nil {
		return false, err
	}

	return true, nil
}

// RecordFixupAttempt increments the fixup attempt counter for a blocked item.
func RecordFixupAttempt(queuePath string, itemID string, last string) (bool, int, error) {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return false, 0, err
	}
	defer lock.Release()

	if err := requireFile(queuePath); err != nil {
		return false, 0, err
	}

	lines, err := readLines(queuePath)
	if err != nil {
		return false, 0, err
	}

	blocks := splitBlocks(lines)
	newBlocks := make([][]string, 0, len(blocks))
	inBlocked := false
	updated := false
	attempts := 0

	for _, block := range blocks {
		header := firstLine(block)
		switch {
		case strings.TrimSpace(header) == "## Blocked":
			inBlocked = true
			newBlocks = append(newBlocks, block)
			continue
		case strings.HasPrefix(header, "## "):
			inBlocked = false
			newBlocks = append(newBlocks, block)
			continue
		}

		if inBlocked && strings.HasPrefix(header, "- [") && extractID(header) == itemID {
			item := parseBlockedItem(block)
			attempts = item.FixupAttempts + 1
			block = updateFixupMetadata(block, attempts, last)
			updated = true
		}
		newBlocks = append(newBlocks, block)
	}

	if !updated {
		return false, 0, nil
	}

	flattened := flattenBlocks(newBlocks)
	if err := writeLines(queuePath, flattened); err != nil {
		return false, 0, err
	}

	return true, attempts, nil
}

// ResetFixupMetadata removes fixup metadata lines from a blocked item.
// found indicates the item existed in Blocked; changed indicates if any metadata was removed.
func ResetFixupMetadata(queuePath string, itemID string) (bool, bool, error) {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return false, false, err
	}
	defer lock.Release()

	if err := requireFile(queuePath); err != nil {
		return false, false, err
	}

	lines, err := readLines(queuePath)
	if err != nil {
		return false, false, err
	}

	blocks := splitBlocks(lines)
	newBlocks := make([][]string, 0, len(blocks))
	inBlocked := false
	found := false
	changed := false

	for _, block := range blocks {
		header := firstLine(block)
		switch {
		case strings.TrimSpace(header) == "## Blocked":
			inBlocked = true
			newBlocks = append(newBlocks, block)
			continue
		case strings.HasPrefix(header, "## "):
			inBlocked = false
			newBlocks = append(newBlocks, block)
			continue
		}

		if inBlocked && strings.HasPrefix(header, "- [") && extractID(header) == itemID {
			found = true
			cleaned, stripped := stripFixupMetadata(block)
			if stripped {
				changed = true
				block = cleaned
			}
		}
		newBlocks = append(newBlocks, block)
	}

	if !found {
		return false, false, nil
	}
	if !changed {
		return true, false, nil
	}

	flattened := flattenBlocks(newBlocks)
	if err := writeLines(queuePath, flattened); err != nil {
		return true, false, err
	}

	return true, true, nil
}

// ToggleQueueItemChecked flips the checked state for a queue item by ID.
func ToggleQueueItemChecked(queuePath string, itemID string) (bool, bool, error) {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return false, false, err
	}
	defer lock.Release()

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
			checked = isHeaderChecked(header)
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

func parseCheckboxHeader(header string) (indent string, tail string, checked bool, ok bool) {
	trimmed := strings.TrimLeft(header, " \t")
	indent = header[:len(header)-len(trimmed)]
	if !strings.HasPrefix(trimmed, "- [") {
		return "", "", false, false
	}
	rest := strings.TrimPrefix(trimmed, "- [")
	closeIdx := strings.Index(rest, "]")
	if closeIdx == -1 {
		return "", "", false, false
	}
	checkbox := rest[:closeIdx]
	normalized := strings.TrimSpace(checkbox)
	if normalized != "" && !strings.EqualFold(normalized, "x") {
		return "", "", false, false
	}
	tail = rest[closeIdx+1:]
	return indent, tail, strings.EqualFold(normalized, "x"), true
}

func isHeaderChecked(header string) bool {
	_, _, checked, ok := parseCheckboxHeader(header)
	return ok && checked
}

// TrimCheckboxPrefix removes the leading checkbox marker from a queue header.
func TrimCheckboxPrefix(header string) string {
	_, tail, _, ok := parseCheckboxHeader(header)
	if !ok {
		return strings.TrimSpace(header)
	}
	return strings.TrimSpace(tail)
}

func toggleCheckHeader(header string) string {
	indent, tail, checked, ok := parseCheckboxHeader(header)
	if !ok {
		return header
	}
	if checked {
		return indent + "- [ ]" + tail
	}
	return indent + "- [x]" + tail
}

func appendMetadata(block []string, reasonLines []string, metadata Metadata) []string {
	for _, line := range reasonLines {
		clean := strings.TrimSpace(line)
		if clean != "" {
			block = append(block, fmt.Sprintf("%s%s %s", metadataIndent, blockedReasonPrefix, clean))
		}
	}
	if metadata.WIPBranch != "" {
		block = append(block, fmt.Sprintf("%s%s %s", metadataIndent, wipBranchPrefix, metadata.WIPBranch))
	}
	if metadata.KnownGood != "" {
		block = append(block, fmt.Sprintf("%s%s %s", metadataIndent, knownGoodPrefix, metadata.KnownGood))
	}
	if metadata.UnblockHint != "" {
		block = append(block, fmt.Sprintf("%s%s %s", metadataIndent, unblockHintPrefix, metadata.UnblockHint))
	}
	return block
}

// RequeueOptions controls how a blocked item is requeued.
type RequeueOptions struct {
	InsertAtTop bool
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
		bodyErrors := make([]string, 0)
		check := validateQueueItemLines(itemLines)
		evidenceOk = check.Evidence
		planOk = check.Plan
		bodyErrors = append(bodyErrors, check.Errors...)

		if !(idOk && tagOk && colonOk && scopeOk && evidenceOk && planOk) || len(bodyErrors) > 0 {
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
			output = append(output, bodyErrors...)
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

type queueItemLineCheck struct {
	Evidence bool
	Plan     bool
	Errors   []string
}

func validateQueueItemLines(lines []string) queueItemLineCheck {
	check := queueItemLineCheck{
		Errors: make([]string, 0),
	}
	inFence := false
	unsafeList := false
	unsafeHeader := false
	fenceIndent := false
	fenceLang := false
	fenceContentIndent := false

	for _, line := range lines {
		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "- Evidence:") {
			check.Evidence = true
		}
		if strings.HasPrefix(trimmed, "- Plan:") {
			check.Plan = true
		}

		if strings.HasPrefix(line, "- [") {
			unsafeList = true
		}
		if strings.HasPrefix(line, "## ") {
			unsafeHeader = true
		}

		if strings.HasPrefix(trimmed, "```") {
			if line == trimmed {
				fenceIndent = true
			}
			fenceLanguage := strings.TrimSpace(strings.TrimPrefix(trimmed, "```"))
			if !inFence {
				if fenceLanguage != "yaml" && fenceLanguage != "yml" {
					fenceLang = true
				}
				inFence = true
			} else {
				inFence = false
			}
			continue
		}

		if inFence && trimmed != "" && line == trimmed {
			fenceContentIndent = true
		}
	}

	if unsafeList {
		check.Errors = append(check.Errors, "  - Unindented list items starting with \"- [\" are not allowed inside queue items. Indent extra metadata bullets by two spaces.")
	}
	if unsafeHeader {
		check.Errors = append(check.Errors, "  - Unindented subheaders (\"## \") are not allowed inside queue items. Indent extra metadata headers by two spaces.")
	}
	if fenceIndent {
		check.Errors = append(check.Errors, "  - Fenced metadata blocks must be indented by two spaces.")
	}
	if fenceLang {
		check.Errors = append(check.Errors, "  - Only indented ```yaml or ```yml fenced blocks are supported for extra metadata.")
	}
	if fenceContentIndent {
		check.Errors = append(check.Errors, "  - Lines inside fenced metadata blocks must be indented by two spaces.")
	}
	if inFence {
		check.Errors = append(check.Errors, "  - Fenced metadata blocks must be closed with an indented ``` line.")
	}
	return check
}

func splitBlocks(lines []string) [][]string {
	blocks := make([][]string, 0)
	current := make([]string, 0)

	for _, line := range lines {
		if queueItemLine.MatchString(line) || strings.HasPrefix(line, "## ") {
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

func parseBlockedItem(block []string) BlockedItem {
	item := BlockedItem{
		Header: firstLine(block),
		Lines:  block,
		ID:     extractID(firstLine(block)),
	}
	metadata := Metadata{}
	attempts := 0
	last := ""

	for _, line := range block[1:] {
		trimmed := strings.TrimLeft(line, " \t")
		switch {
		case strings.HasPrefix(trimmed, wipBranchPrefix):
			metadata.WIPBranch = strings.TrimSpace(strings.TrimPrefix(trimmed, wipBranchPrefix))
		case strings.HasPrefix(trimmed, knownGoodPrefix):
			metadata.KnownGood = strings.TrimSpace(strings.TrimPrefix(trimmed, knownGoodPrefix))
		case strings.HasPrefix(trimmed, unblockHintPrefix):
			metadata.UnblockHint = strings.TrimSpace(strings.TrimPrefix(trimmed, unblockHintPrefix))
		case strings.HasPrefix(trimmed, fixupAttemptsPrefix):
			if value, ok := parseFixupAttempts(trimmed); ok {
				attempts = value
			}
		case strings.HasPrefix(trimmed, fixupLastPrefix):
			last = strings.TrimSpace(strings.TrimPrefix(trimmed, fixupLastPrefix))
		}
	}

	item.Metadata = metadata
	item.FixupAttempts = attempts
	item.FixupLast = last
	return item
}

func parseFixupAttempts(trimmed string) (int, bool) {
	value := strings.TrimSpace(strings.TrimPrefix(trimmed, fixupAttemptsPrefix))
	if value == "" {
		return 0, false
	}
	var attempts int
	if _, err := fmt.Sscanf(value, "%d", &attempts); err != nil {
		return 0, false
	}
	return attempts, true
}

func updateFixupMetadata(block []string, attempts int, last string) []string {
	attemptLine := fmt.Sprintf("%s%s %d", metadataIndent, fixupAttemptsPrefix, attempts)
	lastLine := fmt.Sprintf("%s%s %s", metadataIndent, fixupLastPrefix, last)

	updatedAttempts := false
	updatedLast := false
	for i, line := range block {
		trimmed := strings.TrimLeft(line, " \t")
		switch {
		case strings.HasPrefix(trimmed, fixupAttemptsPrefix):
			block[i] = attemptLine
			updatedAttempts = true
		case strings.HasPrefix(trimmed, fixupLastPrefix):
			if last != "" {
				block[i] = lastLine
				updatedLast = true
			}
		}
	}

	if !updatedAttempts {
		block = append(block, attemptLine)
	}
	if last != "" && !updatedLast {
		block = append(block, lastLine)
	}
	return block
}

func hasAnyPrefix(value string, prefixes ...string) bool {
	for _, prefix := range prefixes {
		if strings.HasPrefix(value, prefix) {
			return true
		}
	}
	return false
}

func stripBlockedMetadata(block []string) []string {
	cleaned := make([]string, 0, len(block))
	for i, line := range block {
		if i == 0 {
			cleaned = append(cleaned, line)
			continue
		}
		trimmed := strings.TrimLeft(line, " \t")
		if hasAnyPrefix(trimmed,
			blockedReasonPrefix,
			wipBranchPrefix,
			knownGoodPrefix,
			unblockHintPrefix,
			fixupAttemptsPrefix,
			fixupLastPrefix,
		) {
			continue
		}
		cleaned = append(cleaned, line)
	}
	return cleaned
}

func stripFixupMetadata(block []string) ([]string, bool) {
	cleaned := make([]string, 0, len(block))
	changed := false
	for i, line := range block {
		if i == 0 {
			cleaned = append(cleaned, line)
			continue
		}
		trimmed := strings.TrimLeft(line, " \t")
		if hasAnyPrefix(trimmed, fixupAttemptsPrefix, fixupLastPrefix) {
			changed = true
			continue
		}
		cleaned = append(cleaned, line)
	}
	return cleaned, changed
}

func setHeaderUnchecked(header string) string {
	indent, tail, _, ok := parseCheckboxHeader(header)
	if !ok {
		return header
	}
	return indent + "- [ ]" + tail
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
	return fileutil.WriteFileAtomic(path, []byte(payload), 0o600)
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
