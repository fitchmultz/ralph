// Package tui provides unified search helpers for navigation and pin entries.
package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

type navKeyedItem interface {
	list.Item
	navKey() string
}

func navKeyOf(item list.Item) string {
	if item == nil {
		return ""
	}
	keyed, ok := item.(navKeyedItem)
	if !ok {
		return ""
	}
	return keyed.navKey()
}

type pinResultItem struct {
	section pinSection
	id      string
	header  string
}

func (p pinResultItem) Title() string {
	return p.id
}

func (p pinResultItem) Description() string {
	title := trimTitle(p.header)
	if title == "" {
		return p.section.Label()
	}
	return fmt.Sprintf("%s | %s", p.section.Label(), title)
}

func (p pinResultItem) FilterValue() string {
	return strings.TrimSpace(fmt.Sprintf("%s %s", p.id, p.header))
}

func (p pinResultItem) navKey() string {
	return fmt.Sprintf("pin:%s:%s", strings.ToLower(p.section.Label()), strings.ToLower(p.id))
}

func searchParts(term string) []string {
	trimmed := strings.TrimSpace(term)
	if trimmed == "" {
		return nil
	}
	return strings.Fields(strings.ToLower(trimmed))
}

func matchesAll(haystack string, parts []string) bool {
	if len(parts) == 0 {
		return true
	}
	lower := strings.ToLower(haystack)
	for _, part := range parts {
		if !strings.Contains(lower, part) {
			return false
		}
	}
	return true
}

func matchesNavItem(item navItem, parts []string) bool {
	if len(parts) == 0 {
		return true
	}
	haystack := strings.Join([]string{item.title, item.desc, screenName(item.screen)}, " ")
	return matchesAll(haystack, parts)
}

func matchesPinEntryForSearch(item pinTableEntry, parts []string) bool {
	if len(parts) == 0 {
		return true
	}
	tags := strings.Join(pin.ExtractTags(item.Header), " ")
	haystack := strings.Join([]string{item.ID, trimTitle(item.Header), tags}, " ")
	return matchesAll(haystack, parts)
}
