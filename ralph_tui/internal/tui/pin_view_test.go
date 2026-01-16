// Package tui provides tests for pin view reload behavior.
package tui

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

func TestPinReloadAsyncSetsLoading(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	cmd := view.reloadAsync(true)
	if cmd == nil {
		t.Fatalf("expected reloadAsync to return a command")
	}
	if !view.loading {
		t.Fatalf("expected reloadAsync to set loading")
	}
}

func TestPinReloadAsyncQueuesWhenBusy(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	view.loading = true
	cmd := view.reloadAsync(true)
	if cmd != nil {
		t.Fatalf("expected nil command when already loading")
	}
	if !view.reloadAgain {
		t.Fatalf("expected reloadAgain to be set when already loading")
	}
}

func TestPinReloadAsyncClearsReloadAgainOnStart(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	view.reloadAgain = true
	cmd := view.reloadAsync(true)
	if cmd == nil {
		t.Fatalf("expected reloadAsync to return a command")
	}
	if view.reloadAgain {
		t.Fatalf("expected reloadAgain to clear when starting reload")
	}
}

func TestPinReloadAsyncClearsReloadAgainOnError(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	view.loading = true
	view.reloadAgain = true
	_ = view.Update(pinReloadMsg{err: errSentinel}, newTestKeyMap(), loopIdle)
	if view.reloadAgain {
		t.Fatalf("expected reloadAgain to clear on reload error")
	}
	if view.loading {
		t.Fatalf("expected loading to clear on reload error")
	}
}

func TestPinReloadPreservesSelectionAndScrollWhenItemRemains(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	initialItems := []pin.QueueItem{
		{ID: "RQ-001", Lines: []string{"- [ ] RQ-001", "a", "b"}},
		{ID: "RQ-002", Lines: []string{"- [ ] RQ-002", "one", "two", "three", "four"}},
	}
	view.setItems(initialItems, nil, "", 0, true)
	view.table.SetCursor(1)
	view.detail.Height = 2
	view.syncDetail(true)
	view.detail.SetYOffset(2)

	reloadedItems := []pin.QueueItem{
		{ID: "RQ-002", Lines: []string{"- [ ] RQ-002", "one", "two", "three", "four"}},
		{ID: "RQ-003", Lines: []string{"- [ ] RQ-003", "x"}},
	}
	_ = view.Update(
		pinReloadMsg{queueItems: reloadedItems, queueStamp: view.queueStamp},
		newTestKeyMap(),
		loopIdle,
	)

	if item := view.selectedItem(); item == nil || item.ID != "RQ-002" {
		t.Fatalf("expected selection to remain on RQ-002, got %+v", item)
	}
	if view.table.Cursor() != 0 {
		t.Fatalf("expected cursor to move to reselected row, got %d", view.table.Cursor())
	}
	if view.detail.YOffset != 2 {
		t.Fatalf("expected detail scroll to remain at offset 2, got %d", view.detail.YOffset)
	}
}

func TestPinReloadClampsCursorWhenRowsShrink(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	initialItems := []pin.QueueItem{
		{ID: "RQ-010", Lines: []string{"- [ ] RQ-010"}},
		{ID: "RQ-011", Lines: []string{"- [ ] RQ-011"}},
		{ID: "RQ-012", Lines: []string{"- [ ] RQ-012"}},
	}
	view.setItems(initialItems, nil, "", 0, true)
	view.table.SetCursor(2)

	reloadedItems := []pin.QueueItem{
		{ID: "RQ-100", Lines: []string{"- [ ] RQ-100"}},
	}
	_ = view.Update(
		pinReloadMsg{queueItems: reloadedItems, queueStamp: view.queueStamp},
		newTestKeyMap(),
		loopIdle,
	)

	if view.table.Cursor() != 0 {
		t.Fatalf("expected cursor to clamp to 0, got %d", view.table.Cursor())
	}
	if view.selectedItem() == nil {
		t.Fatalf("expected selection to exist after clamping")
	}
}

func TestPinSearchEntriesIncludesQueueAndBlocked(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	queueItems := []pin.QueueItem{
		{ID: "RQ-010", Header: "- [ ] RQ-010 [ui]: Alpha", Lines: []string{"- [ ] RQ-010 [ui]: Alpha"}},
	}
	blockedItems := []pin.BlockedItem{
		{ID: "RQ-900", Header: "- [ ] RQ-900 [ops]: Blocked", Lines: []string{"- [ ] RQ-900 [ops]: Blocked"}},
	}
	view.setItems(queueItems, blockedItems, "", 0, true)

	entries := view.SearchEntries()
	if len(entries) != 2 {
		t.Fatalf("expected 2 search entries, got %d", len(entries))
	}
	if entries[0].Section != pinSectionQueue || entries[0].ID != "RQ-010" {
		t.Fatalf("expected queue entry first, got %+v", entries[0])
	}
	if entries[1].Section != pinSectionBlocked || entries[1].ID != "RQ-900" {
		t.Fatalf("expected blocked entry second, got %+v", entries[1])
	}
}

func TestPinSelectItemSwitchesSection(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	queueItems := []pin.QueueItem{
		{ID: "RQ-010", Header: "- [ ] RQ-010 [ui]: Alpha", Lines: []string{"- [ ] RQ-010 [ui]: Alpha"}},
	}
	blockedItems := []pin.BlockedItem{
		{ID: "RQ-900", Header: "- [ ] RQ-900 [ops]: Blocked", Lines: []string{"- [ ] RQ-900 [ops]: Blocked"}},
	}
	view.setItems(queueItems, blockedItems, "", 0, true)

	if !view.SelectItem(pinSectionBlocked, "RQ-900") {
		t.Fatalf("expected SelectItem to succeed")
	}
	if view.section != pinSectionBlocked {
		t.Fatalf("expected section to switch to blocked, got %v", view.section)
	}
	if view.selectedItemID() != "RQ-900" {
		t.Fatalf("expected selection to move to RQ-900, got %q", view.selectedItemID())
	}
}

func TestPinSelectItemByIDSwitchesToQueue(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	queueItems := []pin.QueueItem{
		{ID: "RQ-010", Header: "- [ ] RQ-010 [ui]: Alpha", Lines: []string{"- [ ] RQ-010 [ui]: Alpha"}},
	}
	blockedItems := []pin.BlockedItem{
		{ID: "RQ-900", Header: "- [ ] RQ-900 [ops]: Blocked", Lines: []string{"- [ ] RQ-900 [ops]: Blocked"}},
	}
	view.setItems(queueItems, blockedItems, "", 0, true)
	view.section = pinSectionBlocked

	if !view.SelectItemByID("RQ-010") {
		t.Fatalf("expected SelectItemByID to succeed")
	}
	if view.section != pinSectionQueue {
		t.Fatalf("expected section to switch to queue, got %v", view.section)
	}
	if view.selectedItemID() != "RQ-010" {
		t.Fatalf("expected selection to move to RQ-010, got %q", view.selectedItemID())
	}
}

func TestPinToggleSectionShowsBlockedItems(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	queueItems := []pin.QueueItem{
		{ID: "RQ-010", Header: "- [ ] RQ-010 [ui]: Alpha", Lines: []string{"- [ ] RQ-010 [ui]: Alpha"}},
	}
	blockedItems := []pin.BlockedItem{
		{
			ID:            "RQ-900",
			Header:        "- [ ] RQ-900 [code]: Blocked",
			Lines:         []string{"- [ ] RQ-900 [code]: Blocked"},
			FixupAttempts: 2,
		},
	}
	_ = view.Update(
		pinReloadMsg{queueItems: queueItems, blockedItems: blockedItems, blockedCount: len(blockedItems)},
		newTestKeyMap(),
		loopIdle,
	)

	_ = view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("B")}, newTestKeyMap(), loopIdle)

	if view.section != pinSectionBlocked {
		t.Fatalf("expected section to be blocked, got %v", view.section)
	}
	if len(view.items) != 1 {
		t.Fatalf("expected 1 blocked item, got %d", len(view.items))
	}
	if view.items[0].Section != pinSectionBlocked {
		t.Fatalf("expected blocked items in view, got %v", view.items[0].Section)
	}
	if !strings.Contains(view.statusLine(), "Blocked") {
		t.Fatalf("expected status line to mention blocked, got %q", view.statusLine())
	}
}

func TestPinMoveCheckedDefaultsToPrepend(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	queueContent := strings.Join([]string{
		"## Queue",
		"- [x] RQ-010 [ui]: First",
		"  - Evidence: test",
		"  - Plan: test",
		"- [x] RQ-011 [code]: Second",
		"  - Evidence: test",
		"  - Plan: test",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	doneContent := strings.Join([]string{
		"## Done",
		"- [x] RQ-0009 [code]: Existing done",
		"  - Evidence: done",
		"  - Plan: done",
		"",
	}, "\n")

	writeTestFile(t, view.files.QueuePath, queueContent)
	writeTestFile(t, view.files.DonePath, doneContent)

	if err := view.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}

	view.startMoveChecked()
	if !view.movePrepend {
		t.Fatalf("expected movePrepend default true")
	}
	_ = view.finishMoveChecked()

	doneData := string(mustReadFile(t, view.files.DonePath))
	idxMoved := strings.Index(doneData, "RQ-010")
	idxExisting := strings.Index(doneData, "RQ-0009")
	if idxMoved == -1 || idxExisting == -1 {
		t.Fatalf("expected done content to include moved and existing items")
	}
	if idxMoved > idxExisting {
		t.Fatalf("expected moved items prepended before existing done")
	}
}

func TestPinMoveCheckedAppendChoice(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	queueContent := strings.Join([]string{
		"## Queue",
		"- [x] RQ-010 [ui]: First",
		"  - Evidence: test",
		"  - Plan: test",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	doneContent := strings.Join([]string{
		"## Done",
		"- [x] RQ-0009 [code]: Existing done",
		"  - Evidence: done",
		"  - Plan: done",
		"",
	}, "\n")

	writeTestFile(t, view.files.QueuePath, queueContent)
	writeTestFile(t, view.files.DonePath, doneContent)

	if err := view.reload(); err != nil {
		t.Fatalf("reload pin view: %v", err)
	}

	view.startMoveChecked()
	view.movePrepend = false
	_ = view.finishMoveChecked()

	doneData := string(mustReadFile(t, view.files.DonePath))
	idxExisting := strings.Index(doneData, "RQ-0009")
	idxMoved := strings.Index(doneData, "RQ-010")
	if idxMoved == -1 || idxExisting == -1 {
		t.Fatalf("expected done content to include moved and existing items")
	}
	if idxExisting > idxMoved {
		t.Fatalf("expected existing done to remain before appended items")
	}
}
