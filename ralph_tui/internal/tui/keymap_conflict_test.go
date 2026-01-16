// Package tui provides keymap policy and conflict tests.
package tui

import (
	"strings"
	"testing"

	"github.com/charmbracelet/bubbles/help"
	"github.com/charmbracelet/bubbles/key"
)

func TestKeymapContextsHaveNoConflicts(t *testing.T) {
	keys := newTestKeyMap()
	contexts := []struct {
		name string
		km   help.KeyMap
	}{
		{name: "global", km: globalKeyMap{keys: keys}},
		{name: "search", km: searchKeyMap{keys: keys}},
		{name: "dashboard", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: dashboardKeyMap{keys: keys}}},
		{name: "config", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: configKeyMap{keys: keys}}},
		{name: "pin", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: pinKeyMap{keys: keys}}},
		{name: "logs", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: logsKeyMap{keys: keys}}},
		{name: "specs-idle", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: specsKeyMap{keys: keys, running: false}}},
		{name: "specs-running", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: specsKeyMap{keys: keys, running: true}}},
		{name: "loop-idle-no-effort", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: loopKeyMap{keys: keys, mode: loopIdle, supportsEffort: false}}},
		{name: "loop-idle-effort", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: loopKeyMap{keys: keys, mode: loopIdle, supportsEffort: true}}},
		{name: "loop-running", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: loopKeyMap{keys: keys, mode: loopRunning, supportsEffort: true}}},
		{name: "loop-stopping", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: loopKeyMap{keys: keys, mode: loopStopping, supportsEffort: true}}},
		{name: "loop-editing", km: mergedKeyMap{global: globalKeyMap{keys: keys}, screen: loopKeyMap{keys: keys, mode: loopEditing, supportsEffort: true}}},
	}

	for _, ctx := range contexts {
		t.Run(ctx.name, func(t *testing.T) {
			bindings := flattenBindings(ctx.km.FullHelp())
			assertNoKeyConflicts(t, ctx.name, bindings)
		})
	}
}

func TestGlobalKeyPolicy_GlobalActionsAreCtrlCombos(t *testing.T) {
	keys := newTestKeyMap()
	bindings := []key.Binding{
		keys.Quit,
		keys.ToggleNav,
		keys.Focus,
		keys.Help,
		keys.RefreshNow,
		keys.Search,
	}
	for _, binding := range bindings {
		for _, keyName := range binding.Keys() {
			if !strings.HasPrefix(keyName, "ctrl+") {
				t.Fatalf("expected global key %q to use ctrl+ combo", keyName)
			}
		}
	}
}

func flattenBindings(groups [][]key.Binding) []key.Binding {
	bindings := make([]key.Binding, 0)
	for _, group := range groups {
		bindings = append(bindings, group...)
	}
	return bindings
}

func assertNoKeyConflicts(t *testing.T, context string, bindings []key.Binding) {
	t.Helper()
	seen := make(map[string]string)
	for _, binding := range bindings {
		help := binding.Help()
		desc := help.Desc
		if desc == "" {
			desc = help.Key
		}
		for _, keyName := range binding.Keys() {
			if keyName == "" {
				continue
			}
			if prior, ok := seen[keyName]; ok {
				t.Fatalf("key conflict in %s: %q used by %q and %q", context, keyName, prior, desc)
			}
			seen[keyName] = desc
		}
	}
}
