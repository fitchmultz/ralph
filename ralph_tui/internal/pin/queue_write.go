package pin

import (
	"fmt"
	"path/filepath"
	"strings"
)

// InsertQueueOptions controls where queue items are inserted.
type InsertQueueOptions struct {
	InsertAtTop bool
}

// InsertQueueItem inserts a queue item block into the Queue section.
func InsertQueueItem(queuePath string, itemBlock []string, opts InsertQueueOptions) error {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return err
	}
	defer lock.Release()

	if err := requireFile(queuePath); err != nil {
		return err
	}
	if err := validateQueueItemBlock(itemBlock); err != nil {
		return err
	}

	lines, err := readLines(queuePath)
	if err != nil {
		return err
	}
	if err := ensureQueueSections(lines); err != nil {
		return err
	}

	blocks := splitBlocks(lines)
	queueIndex := -1
	for idx, block := range blocks {
		if strings.TrimSpace(firstLine(block)) == "## Queue" {
			queueIndex = idx
			break
		}
	}
	if queueIndex < 0 {
		return fmt.Errorf("Queue section not found while inserting item.")
	}

	insertPos := queueIndex + 1
	if !opts.InsertAtTop {
		insertPos = findSectionEnd(blocks, queueIndex)
	}
	blocks = insertBlocks(blocks, insertPos, itemBlock)

	return writeLines(queuePath, flattenBlocks(blocks))
}

// MoveQueueItemToDone moves a queue item by ID into the Done file and marks it checked.
func MoveQueueItemToDone(queuePath string, donePath string, itemID string, opts DoneWriteOptions) (bool, error) {
	lock, err := acquirePinLock(filepath.Dir(queuePath))
	if err != nil {
		return false, err
	}
	defer lock.Release()

	if err := requireFile(queuePath); err != nil {
		return false, err
	}
	if err := requireFile(donePath); err != nil {
		return false, err
	}

	queueLines, err := readLines(queuePath)
	if err != nil {
		return false, err
	}
	doneLines, err := readLines(donePath)
	if err != nil {
		return false, err
	}

	blocks := splitBlocks(queueLines)
	newBlocks := make([][]string, 0, len(blocks))
	inQueue := false
	queueIndex := -1
	var itemBlock []string

	for _, block := range blocks {
		header := firstLine(block)
		switch {
		case strings.TrimSpace(header) == "## Queue":
			inQueue = true
			queueIndex = len(newBlocks)
			newBlocks = append(newBlocks, block)
			continue
		case strings.HasPrefix(header, "## "):
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
	if queueIndex < 0 {
		return false, fmt.Errorf("Queue section not found while moving item.")
	}

	itemBlock[0] = setHeaderChecked(itemBlock[0])
	doneIndex, updated := ensureDoneSection(doneLines)
	doneLines = updated
	insertPos := doneIndex + 1

	if opts.Prepend {
		doneLines = insertLines(doneLines, insertPos, itemBlock)
	} else {
		sectionEnd := len(doneLines)
		for i := doneIndex + 1; i < len(doneLines); i++ {
			if strings.HasPrefix(doneLines[i], "## ") {
				sectionEnd = i
				break
			}
		}
		doneLines = insertLines(doneLines, sectionEnd, itemBlock)
	}

	if err := writeLines(donePath, doneLines); err != nil {
		return false, err
	}

	if err := writeLines(queuePath, flattenBlocks(newBlocks)); err != nil {
		return false, err
	}

	if _, err := TrimDoneItems(donePath, DoneTrimOptions{
		Limit:       opts.RetentionLimit,
		NewestAtTop: opts.Prepend,
	}); err != nil {
		return false, err
	}

	return true, nil
}

func validateQueueItemBlock(block []string) error {
	if len(block) == 0 {
		return fmt.Errorf("queue item block is empty")
	}
	header := firstLine(block)
	if !queueItemLine.MatchString(header) {
		return fmt.Errorf("queue item header must start with '- [ ]' or '- [x]'")
	}
	headerErrors := make([]string, 0)
	if extractID(header) == "" {
		headerErrors = append(headerErrors, "missing ID like RQ-0123")
	}
	if !tagPattern.MatchString(header) {
		headerErrors = append(headerErrors, "missing routing tag like [code]/[code-*]/[db]/[ui]/[ops]/[docs]/[docs-*]")
	}
	if !strings.Contains(header, ": ") {
		headerErrors = append(headerErrors, "missing ':' after ID/tags")
	}
	if !scopePattern.MatchString(header) {
		headerErrors = append(headerErrors, "missing trailing scope list in parentheses")
	}
	if len(headerErrors) > 0 {
		return fmt.Errorf("queue item header invalid: %s", strings.Join(headerErrors, "; "))
	}

	check := validateQueueItemLines(block[1:])
	bodyErrors := make([]string, 0)
	if !check.Evidence {
		bodyErrors = append(bodyErrors, "missing indented metadata bullet: '- Evidence:'")
	}
	if !check.Plan {
		bodyErrors = append(bodyErrors, "missing indented metadata bullet: '- Plan:'")
	}
	bodyErrors = append(bodyErrors, check.Errors...)
	if len(bodyErrors) > 0 {
		return fmt.Errorf("queue item metadata invalid: %s", strings.Join(bodyErrors, "; "))
	}
	return nil
}
