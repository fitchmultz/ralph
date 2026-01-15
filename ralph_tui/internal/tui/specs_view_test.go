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
