// Package tui provides tests for specs view preview refresh behavior.
package tui

import (
	"fmt"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

type fakePreviewRenderer struct {
	renderCalls int
	output      string
}

func (f *fakePreviewRenderer) Render(input string) (string, error) {
	f.renderCalls++
	if f.output == "" {
		return "rendered:" + input, nil
	}
	return f.output, nil
}

func setSpecsRunOutput(view *specsView, lines []string) {
	view.runLogBuf.Reset()
	view.runLogBuf.AppendLines(lines)
	view.finalizeRunOutput()
}

func TestSpecsPreviewRefreshSetsLoading(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}
	view.previewDirty = true

	cmd := view.refreshPreviewAsync()
	if cmd == nil {
		t.Fatalf("expected refreshPreviewAsync to return a command")
	}
	if !view.previewLoading {
		t.Fatalf("expected previewLoading to be true")
	}
	if view.previewDirty {
		t.Fatalf("expected previewDirty to be false")
	}
}

func TestSpecsPreviewRendererCachesByWidth(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	buildCalls := 0
	renderers := map[int]*fakePreviewRenderer{}
	view.rendererBuilder = func(width int) (previewRenderer, error) {
		buildCalls++
		renderer := &fakePreviewRenderer{output: fmt.Sprintf("rendered-%d", width)}
		renderers[width] = renderer
		return renderer, nil
	}

	view.previewWidth = 80
	setSpecsRunOutput(view, []string{"first"})
	cmd := view.refreshPreviewAsync()
	if cmd == nil {
		t.Fatalf("expected refresh command")
	}
	view.Update(cmd().(specsPreviewMsg), newTestKeyMap())

	if buildCalls != 1 {
		t.Fatalf("expected 1 renderer build, got %d", buildCalls)
	}
	if renderers[80].renderCalls != 1 {
		t.Fatalf("expected renderer to render once, got %d", renderers[80].renderCalls)
	}

	setSpecsRunOutput(view, []string{"second"})
	cmd = view.refreshPreviewAsync()
	if cmd == nil {
		t.Fatalf("expected refresh command")
	}
	view.Update(cmd().(specsPreviewMsg), newTestKeyMap())

	if buildCalls != 1 {
		t.Fatalf("expected cached renderer reuse, got %d builds", buildCalls)
	}
	if renderers[80].renderCalls != 2 {
		t.Fatalf("expected renderer to render twice, got %d", renderers[80].renderCalls)
	}

	view.previewWidth = 120
	setSpecsRunOutput(view, []string{"third"})
	cmd = view.refreshPreviewAsync()
	if cmd == nil {
		t.Fatalf("expected refresh command")
	}
	view.Update(cmd().(specsPreviewMsg), newTestKeyMap())

	if buildCalls != 2 {
		t.Fatalf("expected second renderer build after width change, got %d", buildCalls)
	}
	if renderers[120].renderCalls != 1 {
		t.Fatalf("expected new renderer to render once, got %d", renderers[120].renderCalls)
	}
}

func TestSpecsPreviewRendererCacheIsBounded(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	buildCalls := 0
	view.rendererBuilder = func(width int) (previewRenderer, error) {
		buildCalls++
		return &fakePreviewRenderer{output: fmt.Sprintf("rendered-%d", width)}, nil
	}

	baseWidth := 80
	overflow := 3
	for i := 0; i < specsPreviewRendererCacheMaxEntries+overflow; i++ {
		if _, err := view.previewRenderer(baseWidth + i); err != nil {
			t.Fatalf("previewRenderer failed: %v", err)
		}
	}

	expectedBuilds := specsPreviewRendererCacheMaxEntries + overflow
	if buildCalls != expectedBuilds {
		t.Fatalf("expected %d renderer builds, got %d", expectedBuilds, buildCalls)
	}
	if len(view.previewRenderers) != specsPreviewRendererCacheMaxEntries {
		t.Fatalf("expected cache size %d, got %d", specsPreviewRendererCacheMaxEntries, len(view.previewRenderers))
	}
	for i := 0; i < overflow; i++ {
		if _, ok := view.previewRenderers[baseWidth+i]; ok {
			t.Fatalf("expected width %d to be evicted", baseWidth+i)
		}
	}

	if _, err := view.previewRenderer(baseWidth); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}
	if buildCalls != expectedBuilds+1 {
		t.Fatalf("expected rebuild after eviction, got %d builds", buildCalls)
	}
}

func TestSpecsPreviewRendererCacheEvictsLeastRecentlyUsed(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	buildCalls := 0
	view.rendererBuilder = func(width int) (previewRenderer, error) {
		buildCalls++
		return &fakePreviewRenderer{output: fmt.Sprintf("rendered-%d", width)}, nil
	}

	baseWidth := 100
	for i := 0; i < specsPreviewRendererCacheMaxEntries; i++ {
		if _, err := view.previewRenderer(baseWidth + i); err != nil {
			t.Fatalf("previewRenderer failed: %v", err)
		}
	}

	if _, err := view.previewRenderer(baseWidth); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}

	if _, err := view.previewRenderer(baseWidth + specsPreviewRendererCacheMaxEntries); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}

	if _, ok := view.previewRenderers[baseWidth]; !ok {
		t.Fatalf("expected most-recent width %d to remain cached", baseWidth)
	}
	if _, ok := view.previewRenderers[baseWidth+1]; ok {
		t.Fatalf("expected width %d to be evicted", baseWidth+1)
	}

	expectedBuilds := specsPreviewRendererCacheMaxEntries + 1
	if buildCalls != expectedBuilds {
		t.Fatalf("expected %d renderer builds, got %d", expectedBuilds, buildCalls)
	}

	if _, err := view.previewRenderer(baseWidth + 1); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}
	if buildCalls != expectedBuilds+1 {
		t.Fatalf("expected rebuild for evicted width, got %d builds", buildCalls)
	}

	if _, err := view.previewRenderer(baseWidth); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}
	if buildCalls != expectedBuilds+1 {
		t.Fatalf("expected cached width to stay hot, got %d builds", buildCalls)
	}
}

func TestSpecsPreviewRendererCacheClearsOnThemeChange(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	buildCalls := 0
	view.rendererBuilder = func(width int) (previewRenderer, error) {
		buildCalls++
		return &fakePreviewRenderer{output: fmt.Sprintf("rendered-%d", width)}, nil
	}

	if _, err := view.previewRenderer(80); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}
	if buildCalls != 1 {
		t.Fatalf("expected 1 renderer build, got %d", buildCalls)
	}
	if len(view.previewRenderers) != 1 {
		t.Fatalf("expected cache size 1, got %d", len(view.previewRenderers))
	}

	updatedCfg := cfg
	updatedCfg.UI.Theme = cfg.UI.Theme + "-changed"
	view.SetConfig(updatedCfg, locs)

	if len(view.previewRenderers) != 0 {
		t.Fatalf("expected cache to clear on theme change, got %d entries", len(view.previewRenderers))
	}

	if _, err := view.previewRenderer(80); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}
	if buildCalls != 2 {
		t.Fatalf("expected renderer rebuild after theme change, got %d builds", buildCalls)
	}

	if _, err := view.previewRenderer(80); err != nil {
		t.Fatalf("previewRenderer failed: %v", err)
	}
	if buildCalls != 2 {
		t.Fatalf("expected cached renderer reuse, got %d builds", buildCalls)
	}
}

func TestSpecsPreviewSkipsRenderWhenInputsUnchanged(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	renderer := &fakePreviewRenderer{output: "rendered"}
	view.rendererBuilder = func(width int) (previewRenderer, error) {
		return renderer, nil
	}

	view.previewWidth = 80
	setSpecsRunOutput(view, []string{"first"})
	cmd := view.refreshPreviewAsync()
	if cmd == nil {
		t.Fatalf("expected refresh command")
	}
	msg := cmd().(specsPreviewMsg)
	view.Update(msg, newTestKeyMap())

	if renderer.renderCalls != 1 {
		t.Fatalf("expected renderer to render once, got %d", renderer.renderCalls)
	}

	cmd = view.refreshPreviewAsync()
	if cmd == nil {
		t.Fatalf("expected refresh command")
	}
	msg = cmd().(specsPreviewMsg)
	if !msg.unchanged {
		t.Fatalf("expected unchanged preview message")
	}
	view.Update(msg, newTestKeyMap())

	if renderer.renderCalls != 1 {
		t.Fatalf("expected renderer to skip re-render, got %d", renderer.renderCalls)
	}
}

func TestSpecsViewTogglesMarkExplicit(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}
	keys := newTestKeyMap()

	view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("a")}, keys)
	if !view.autofillScout || !view.autofillExplicit {
		t.Fatalf("expected autofill toggle to set explicit state")
	}

	view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("n")}, keys)
	if !view.innovate || !view.innovateExplicit {
		t.Fatalf("expected innovate toggle to set explicit state")
	}
}

func TestSpecsViewConfigReloadResetsExplicitAutofill(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}
	keys := newTestKeyMap()

	view.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("a")}, keys)
	if !view.autofillExplicit {
		t.Fatalf("expected autofill to be explicit after toggle")
	}

	sameCfg := cfg
	view.SetConfig(sameCfg, locs)
	if !view.autofillExplicit {
		t.Fatalf("expected explicit autofill to remain when config unchanged")
	}

	updatedCfg := cfg
	updatedCfg.Specs.AutofillScout = !cfg.Specs.AutofillScout
	view.SetConfig(updatedCfg, locs)
	if view.autofillExplicit {
		t.Fatalf("expected explicit autofill to clear after config change")
	}
	if view.autofillScout != updatedCfg.Specs.AutofillScout {
		t.Fatalf("expected autofill to match updated config")
	}
}

func TestSpecsPreviewDebounceIgnoresStaleMessages(t *testing.T) {
	_, locs, cfg := newHermeticModel(t)
	view, err := newSpecsView(cfg, locs, newTestKeyMap())
	if err != nil {
		t.Fatalf("newSpecsView failed: %v", err)
	}

	renderer := &fakePreviewRenderer{output: "rendered"}
	view.rendererBuilder = func(width int) (previewRenderer, error) {
		return renderer, nil
	}

	view.previewDirty = true
	view.previewLoading = false
	view.resizeDebounce = 0

	cmd1 := view.DebouncedRefreshPreviewCmd()
	cmd2 := view.DebouncedRefreshPreviewCmd()
	if cmd1 == nil || cmd2 == nil {
		t.Fatalf("expected debounce commands")
	}

	msg1 := cmd1().(specsPreviewDebounceMsg)
	if cmd := view.Update(msg1, newTestKeyMap()); cmd != nil {
		t.Fatalf("expected stale debounce to do nothing")
	}
	if view.previewLoading {
		t.Fatalf("expected preview to remain idle after stale debounce")
	}

	msg2 := cmd2().(specsPreviewDebounceMsg)
	cmd := view.Update(msg2, newTestKeyMap())
	if cmd == nil {
		t.Fatalf("expected debounce to trigger preview refresh")
	}
	if !view.previewLoading {
		t.Fatalf("expected preview to start loading after debounce")
	}

	view.Update(cmd().(specsPreviewMsg), newTestKeyMap())
	if renderer.renderCalls != 1 {
		t.Fatalf("expected renderer to run once, got %d", renderer.renderCalls)
	}
}
