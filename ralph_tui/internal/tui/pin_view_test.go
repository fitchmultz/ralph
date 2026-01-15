// Package tui provides tests for pin view reload behavior.
package tui

import (
	"testing"

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
	_ = view.Update(pinReloadMsg{err: errSentinel}, newTestKeyMap())
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
	view.items = initialItems
	view.table.SetRows(makePinRows(initialItems))
	view.table.SetCursor(1)
	view.detail.Height = 2
	view.syncDetail(true)
	view.detail.SetYOffset(2)

	reloadedItems := []pin.QueueItem{
		{ID: "RQ-002", Lines: []string{"- [ ] RQ-002", "one", "two", "three", "four"}},
		{ID: "RQ-003", Lines: []string{"- [ ] RQ-003", "x"}},
	}
	_ = view.Update(
		pinReloadMsg{items: reloadedItems, queueStamp: view.queueStamp},
		newTestKeyMap(),
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
	view.items = initialItems
	view.table.SetRows(makePinRows(initialItems))
	view.table.SetCursor(2)

	reloadedItems := []pin.QueueItem{
		{ID: "RQ-100", Lines: []string{"- [ ] RQ-100"}},
	}
	_ = view.Update(
		pinReloadMsg{items: reloadedItems, queueStamp: view.queueStamp},
		newTestKeyMap(),
	)

	if view.table.Cursor() != 0 {
		t.Fatalf("expected cursor to clamp to 0, got %d", view.table.Cursor())
	}
	if view.selectedItem() == nil {
		t.Fatalf("expected selection to exist after clamping")
	}
}

func TestPinFilterClearsAndRestoresSelection(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newPinView(cfg, locs)
	if err != nil {
		t.Fatalf("newPinView failed: %v", err)
	}

	items := []pin.QueueItem{
		{ID: "RQ-010", Header: "- [ ] RQ-010 [ui]: Alpha", Lines: []string{"- [ ] RQ-010 [ui]: Alpha"}},
		{ID: "RQ-011", Header: "- [ ] RQ-011 [code]: Beta", Lines: []string{"- [ ] RQ-011 [code]: Beta", "detail line", "extra line"}},
		{ID: "RQ-012", Header: "- [ ] RQ-012 [ops]: Gamma", Lines: []string{"- [ ] RQ-012 [ops]: Gamma"}},
	}
	view.setQueueItems(items, "", 0, true)
	view.table.SetCursor(1)
	view.detail.Height = 1
	view.syncDetail(true)
	view.detail.SetYOffset(1)

	if err := view.ApplySearch("ops"); err != nil {
		t.Fatalf("ApplySearch failed: %v", err)
	}
	if len(view.items) != 1 || view.items[0].ID != "RQ-012" {
		t.Fatalf("expected filtered items to include only RQ-012, got %+v", view.items)
	}

	if err := view.ApplySearch(""); err != nil {
		t.Fatalf("ApplySearch clear failed: %v", err)
	}
	if item := view.selectedItem(); item == nil || item.ID != "RQ-011" {
		t.Fatalf("expected selection to restore to RQ-011, got %+v", item)
	}
	if view.detail.YOffset != 1 {
		t.Fatalf("expected detail offset to restore to 1, got %d", view.detail.YOffset)
	}
}
