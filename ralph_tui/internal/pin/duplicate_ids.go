// Package pin provides duplicate queue ID detection and repair helpers.
// Entrypoint: DuplicateIDs, FixDuplicateQueueIDs.
package pin

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/queueid"
)

// DuplicateIDReport summarizes duplicate IDs found across queue and done files.
type DuplicateIDReport struct {
	All     []string
	InQueue []string
	InDone  []string
	Cross   []string
	Fixable []string
}

// DuplicateIDs returns duplicate ID information for queue and done pin files.
func DuplicateIDs(files Files) (DuplicateIDReport, error) {
	if err := requireFile(files.QueuePath); err != nil {
		return DuplicateIDReport{}, err
	}
	if err := requireFile(files.DonePath); err != nil {
		return DuplicateIDReport{}, err
	}

	queueLines, err := readLines(files.QueuePath)
	if err != nil {
		return DuplicateIDReport{}, err
	}
	doneLines, err := readLines(files.DonePath)
	if err != nil {
		return DuplicateIDReport{}, err
	}

	return duplicateIDsFromLines(queueLines, doneLines), nil
}

// DuplicateIDFix captures a single queue ID renumbering.
type DuplicateIDFix struct {
	Section string
	OldID   string
	NewID   string
}

// FixDuplicateIDsResult captures queue ID renumbering results.
type FixDuplicateIDsResult struct {
	Fixed      []DuplicateIDFix
	Duplicates []string
}

// FixDuplicateQueueIDs renumbers duplicate IDs in the queue file without modifying the done log.
func FixDuplicateQueueIDs(files Files, fallbackPrefix string, projectType project.Type) (FixDuplicateIDsResult, error) {
	lock, err := acquirePinLock(filepath.Dir(files.QueuePath))
	if err != nil {
		return FixDuplicateIDsResult{}, err
	}
	defer lock.Release()

	if err := requireFile(files.QueuePath); err != nil {
		return FixDuplicateIDsResult{}, err
	}
	if err := requireFile(files.DonePath); err != nil {
		return FixDuplicateIDsResult{}, err
	}

	queueLines, err := readLines(files.QueuePath)
	if err != nil {
		return FixDuplicateIDsResult{}, err
	}
	doneLines, err := readLines(files.DonePath)
	if err != nil {
		return FixDuplicateIDsResult{}, err
	}

	report := duplicateIDsFromLines(queueLines, doneLines)
	result := FixDuplicateIDsResult{
		Duplicates: report.All,
	}

	if len(report.InDone) > 0 {
		return result, fmt.Errorf("Duplicate task IDs exist in the done log; fix manually: %s", strings.Join(report.InDone, ", "))
	}
	if len(report.Fixable) == 0 {
		return result, nil
	}

	used := make(map[string]struct{})
	for _, id := range extractIDs(doneLines) {
		used[id] = struct{}{}
	}

	allocator := newQueueIDAllocator(append(extractIDs(queueLines), extractIDs(doneLines)...))
	section := ""

	for i, line := range queueLines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "## ") {
			section = strings.TrimPrefix(trimmed, "## ")
		}
		if !queueItemLine.MatchString(line) {
			continue
		}
		id := queueid.Extract(line)
		if id == "" {
			continue
		}
		if _, exists := used[id]; !exists {
			used[id] = struct{}{}
			continue
		}

		nextID := allocator.Next(queueIDPrefix(id, fallbackPrefix))
		queueLines[i] = replaceQueueID(line, id, nextID)
		result.Fixed = append(result.Fixed, DuplicateIDFix{
			Section: section,
			OldID:   id,
			NewID:   nextID,
		})
		used[nextID] = struct{}{}
	}

	if len(result.Fixed) == 0 {
		return result, nil
	}

	if err := writeLines(files.QueuePath, queueLines); err != nil {
		return result, err
	}
	if err := ValidatePin(files, projectType); err != nil {
		return result, err
	}

	return result, nil
}

type queueIDAllocator struct {
	maxByPrefix map[string]int
}

func newQueueIDAllocator(ids []string) *queueIDAllocator {
	maxByPrefix := make(map[string]int)
	for _, id := range ids {
		prefix, number, ok := queueid.Parse(id)
		if !ok {
			continue
		}
		if number > maxByPrefix[prefix] {
			maxByPrefix[prefix] = number
		}
	}
	return &queueIDAllocator{maxByPrefix: maxByPrefix}
}

func (a *queueIDAllocator) Next(prefix string) string {
	normalized := strings.ToUpper(strings.TrimSpace(prefix))
	if normalized == "" {
		normalized = queueid.DefaultPrefix
	}
	next := a.maxByPrefix[normalized] + 1
	a.maxByPrefix[normalized] = next
	return fmt.Sprintf("%s-%04d", normalized, next)
}

func queueIDPrefix(id string, fallback string) string {
	if prefix, _, ok := queueid.Parse(id); ok {
		return prefix
	}
	if fallback != "" {
		return fallback
	}
	return queueid.DefaultPrefix
}

func replaceQueueID(line string, oldID string, newID string) string {
	return strings.Replace(line, oldID, newID, 1)
}

func duplicateIDsFromLines(queueLines []string, doneLines []string) DuplicateIDReport {
	queueIDs := extractIDs(queueLines)
	doneIDs := extractIDs(doneLines)

	queueDupes := findDuplicatesSorted(queueIDs)
	doneDupes := findDuplicatesSorted(doneIDs)
	crossDupes := crossDuplicates(queueIDs, doneIDs)
	fixable := uniqueSorted(append(queueDupes, crossDupes...))
	all := uniqueSorted(append(doneDupes, fixable...))

	return DuplicateIDReport{
		All:     all,
		InQueue: queueDupes,
		InDone:  doneDupes,
		Cross:   crossDupes,
		Fixable: fixable,
	}
}

func findDuplicatesSorted(ids []string) []string {
	if len(ids) == 0 {
		return nil
	}
	sorted := append([]string{}, ids...)
	sort.Strings(sorted)
	return findDuplicates(sorted)
}

func crossDuplicates(queueIDs []string, doneIDs []string) []string {
	if len(queueIDs) == 0 || len(doneIDs) == 0 {
		return nil
	}
	doneSet := make(map[string]struct{}, len(doneIDs))
	for _, id := range doneIDs {
		doneSet[id] = struct{}{}
	}
	dupes := make(map[string]struct{})
	for _, id := range queueIDs {
		if _, ok := doneSet[id]; ok {
			dupes[id] = struct{}{}
		}
	}
	result := make([]string, 0, len(dupes))
	for id := range dupes {
		result = append(result, id)
	}
	sort.Strings(result)
	return result
}

func uniqueSorted(ids []string) []string {
	if len(ids) == 0 {
		return nil
	}
	seen := make(map[string]struct{})
	unique := make([]string, 0, len(ids))
	for _, id := range ids {
		if _, ok := seen[id]; ok {
			continue
		}
		seen[id] = struct{}{}
		unique = append(unique, id)
	}
	sort.Strings(unique)
	return unique
}
