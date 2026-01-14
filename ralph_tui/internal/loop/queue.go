// Package loop provides queue parsing helpers.
// Entrypoint: FirstUncheckedItem, ExtractItemID.
package loop

import (
	"fmt"
	"os"
	"regexp"
	"strings"
)

var (
	idPattern = regexp.MustCompile(`[A-Z0-9]{2,10}-\d{4}`)
)

// QueueItem captures a parsed queue item header and block.
type QueueItem struct {
	Header  string
	Block   []string
	ID      string
	Checked bool
}

// FirstUncheckedItem returns the first unchecked queue item matching tags.
func FirstUncheckedItem(queuePath string, onlyTags []string) (*QueueItem, error) {
	items, err := readQueueItems(queuePath)
	if err != nil {
		return nil, err
	}
	for _, item := range items {
		if item.Checked {
			continue
		}
		if hasTag(item.Header, onlyTags) {
			return &item, nil
		}
	}
	return nil, nil
}

// CurrentItemBlock returns the block for the given item ID.
func CurrentItemBlock(queuePath string, itemID string) (string, error) {
	items, err := readQueueItems(queuePath)
	if err != nil {
		return "", err
	}
	for _, item := range items {
		if item.ID == itemID {
			return strings.Join(item.Block, "\n"), nil
		}
	}
	return "", fmt.Errorf("item %s not found", itemID)
}

// ExtractItemID returns the ID from a queue header line.
func ExtractItemID(line string) string {
	match := idPattern.FindString(line)
	if match == "" {
		return ""
	}
	return match
}

// ExtractItemTitle returns the title portion of a queue item line.
func ExtractItemTitle(line string) string {
	trimmed := strings.TrimSpace(line)
	if strings.HasPrefix(trimmed, "- [") {
		trimmed = strings.TrimSpace(trimmed[5:])
	}
	if id := idPattern.FindString(trimmed); id != "" {
		idx := strings.Index(trimmed, id)
		if idx >= 0 {
			trimmed = strings.TrimSpace(trimmed[idx+len(id):])
		}
	}
	trimmed = strings.TrimSpace(strings.TrimPrefix(trimmed, ":"))
	trimmed = strings.TrimSpace(strings.TrimPrefix(trimmed, ":"))
	if strings.HasPrefix(trimmed, "[") {
		if closing := strings.Index(trimmed, "]"); closing >= 0 {
			trimmed = strings.TrimSpace(trimmed[closing+1:])
		}
	}
	trimmed = strings.TrimSpace(strings.TrimPrefix(trimmed, ":"))
	return strings.TrimSpace(trimmed)
}

func hasTag(line string, tags []string) bool {
	if len(tags) == 0 {
		return true
	}
	for _, tag := range tags {
		tag = strings.TrimSpace(tag)
		tag = strings.TrimPrefix(strings.TrimSuffix(tag, "]"), "[")
		if tag == "" {
			continue
		}
		if strings.Contains(line, "["+tag+"]") {
			return true
		}
	}
	return false
}

func readQueueItems(queuePath string) ([]QueueItem, error) {
	data, err := os.ReadFile(queuePath)
	if err != nil {
		return nil, err
	}
	lines := strings.Split(strings.TrimSuffix(string(data), "\n"), "\n")
	inQueue := false
	items := make([]QueueItem, 0)

	var current []string
	for _, line := range lines {
		if strings.TrimSpace(line) == "## Queue" {
			flushQueueItem(&items, current)
			current = nil
			inQueue = true
			continue
		}
		if strings.HasPrefix(line, "## ") {
			flushQueueItem(&items, current)
			current = nil
			inQueue = false
			continue
		}
		if !inQueue {
			continue
		}
		if strings.HasPrefix(line, "- [") {
			flushQueueItem(&items, current)
			current = []string{line}
			continue
		}
		if len(current) > 0 {
			current = append(current, line)
		}
	}
	flushQueueItem(&items, current)

	return items, nil
}

func flushQueueItem(items *[]QueueItem, block []string) {
	if len(block) == 0 {
		return
	}
	header := block[0]
	*items = append(*items, QueueItem{
		Header:  header,
		Block:   block,
		ID:      ExtractItemID(header),
		Checked: strings.HasPrefix(strings.TrimSpace(header), "- [x]"),
	})
}
