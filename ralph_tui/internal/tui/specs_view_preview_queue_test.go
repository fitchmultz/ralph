// Package tui provides tests for queued specs preview refresh behavior.
package tui

import "testing"

func TestSpecsPreviewQueueRefreshWhenLoading(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	view.previewLoading = true
	view.previewDirty = false

	cmd := view.requestPreviewRefresh()
	if cmd != nil {
		t.Fatalf("expected queued refresh to return nil command while loading")
	}
	if !view.previewDirty {
		t.Fatalf("expected previewDirty to be true when refresh is queued")
	}

	cmd = view.Update(specsPreviewMsg{preview: "ok", effective: false, auto: false}, newTestKeyMap())
	if cmd == nil {
		t.Fatalf("expected refresh command after queued preview completes")
	}
	if !view.previewLoading {
		t.Fatalf("expected previewLoading to be true after queued refresh starts")
	}
	if view.previewDirty {
		t.Fatalf("expected previewDirty to be false after queued refresh starts")
	}
}
