// Package tui provides tests for unified search behavior.
package tui

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
)

func TestUnifiedSearch_SelectsQueueItemByID(t *testing.T) {
	base, _, cfg := newHermeticModel(t)
	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0001 [ui]: First item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"- [ ] RQ-0002 [ops]: Second item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"- [ ] RQ-9999 [code]: Blocked item",
		"  - Blocked reason: test fixture",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(cfg.Paths.PinDir, "implementation_queue.md"), queueContent)

	m := base
	if m.pinView == nil {
		t.Fatalf("pin view missing")
	}
	if err := m.pinView.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)
	m = typeString(t, m, "RQ-0002")

	targetIndex := findPinResultIndex(m.nav.Items(), "RQ-0002", pinSectionQueue)
	if targetIndex < 0 {
		t.Fatalf("expected queue search result for RQ-0002")
	}
	m.nav.Select(targetIndex)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(model)

	if m.searchActive {
		t.Fatalf("expected search to be inactive after selection")
	}
	if m.screen != screenPin {
		t.Fatalf("expected screenPin after selection, got %v", m.screen)
	}
	if m.pinView.section != pinSectionQueue {
		t.Fatalf("expected queue section, got %v", m.pinView.section)
	}
	if m.pinView.selectedItemID() != "RQ-0002" {
		t.Fatalf("expected selected item RQ-0002, got %q", m.pinView.selectedItemID())
	}
	if len(m.nav.Items()) != len(navigationItems()) {
		t.Fatalf("expected nav items restored after search")
	}
	if m.nav.Index() != navIndexForScreen(screenPin) {
		t.Fatalf("expected nav selection to be pin screen after search")
	}
}

func TestUnifiedSearch_SelectsQueueItemByTag(t *testing.T) {
	base, _, cfg := newHermeticModel(t)
	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0100 [ui]: Alpha",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"- [ ] RQ-0101 [ops]: Beta",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(cfg.Paths.PinDir, "implementation_queue.md"), queueContent)

	m := base
	if m.pinView == nil {
		t.Fatalf("pin view missing")
	}
	if err := m.pinView.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)
	m = typeString(t, m, "ops")

	targetIndex := findPinResultIndex(m.nav.Items(), "RQ-0101", pinSectionQueue)
	if targetIndex < 0 {
		t.Fatalf("expected queue search result for tag ops")
	}
	m.nav.Select(targetIndex)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(model)

	if m.screen != screenPin {
		t.Fatalf("expected screenPin after selection, got %v", m.screen)
	}
	if m.pinView.selectedItemID() != "RQ-0101" {
		t.Fatalf("expected selected item RQ-0101, got %q", m.pinView.selectedItemID())
	}
}

func TestUnifiedSearch_SelectsBlockedItem(t *testing.T) {
	base, _, cfg := newHermeticModel(t)
	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0100 [ui]: Alpha",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"- [ ] RQ-9999 [code]: Blocked item",
		"  - Blocked reason: test fixture",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(cfg.Paths.PinDir, "implementation_queue.md"), queueContent)

	m := base
	if m.pinView == nil {
		t.Fatalf("pin view missing")
	}
	if err := m.pinView.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)
	m = typeString(t, m, "RQ-9999")

	targetIndex := findPinResultIndex(m.nav.Items(), "RQ-9999", pinSectionBlocked)
	if targetIndex < 0 {
		t.Fatalf("expected blocked search result for RQ-9999")
	}
	m.nav.Select(targetIndex)

	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(model)

	if m.screen != screenPin {
		t.Fatalf("expected screenPin after selection, got %v", m.screen)
	}
	if m.pinView.section != pinSectionBlocked {
		t.Fatalf("expected blocked section, got %v", m.pinView.section)
	}
	if m.pinView.selectedItemID() != "RQ-9999" {
		t.Fatalf("expected selected item RQ-9999, got %q", m.pinView.selectedItemID())
	}
}

func TestUnifiedSearch_SelectionKeysOnlyMoveNav(t *testing.T) {
	base, _, cfg := newHermeticModel(t)
	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0001 [ui]: First item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"- [ ] RQ-0002 [ui]: Second item (pin_view.go)",
		"  - Evidence: test fixture",
		"  - Plan: test fixture",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writeTestFile(t, filepath.Join(cfg.Paths.PinDir, "implementation_queue.md"), queueContent)

	m := base
	m.switchScreen(screenPin, true)
	if m.pinView == nil {
		t.Fatalf("pin view missing")
	}
	if err := m.pinView.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	m = updated.(model)

	pinCursor := m.pinView.table.Cursor()
	navIndex := m.nav.Index()
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(model)

	if m.nav.Index() == navIndex {
		t.Fatalf("expected nav selection to move during search")
	}
	if m.pinView.table.Cursor() != pinCursor {
		t.Fatalf("expected pin cursor to remain unchanged during search")
	}
}

func typeString(t *testing.T, m model, value string) model {
	t.Helper()
	for _, r := range value {
		updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}})
		m = updated.(model)
	}
	return m
}

func findPinResultIndex(items []list.Item, itemID string, section pinSection) int {
	for idx, item := range items {
		result, ok := item.(pinResultItem)
		if !ok {
			continue
		}
		if result.id == itemID && result.section == section {
			return idx
		}
	}
	return -1
}
