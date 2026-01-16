// Package tui provides scenario-based TUI snapshot and contract tests.
// Entrypoint: go test ./...
package tui

import (
	"testing"
)

func TestScenario_NavigationSnapshotsAndContracts(t *testing.T) {
	withAsciiColorProfile(t, func() {
		m, _, _ := newHermeticModel(t)
		driver := newModelDriver(t, m)
		driver.Resize(80, 24)

		driver.AssertScreen(screenDashboard)
		driver.Snapshot("01_dashboard_initial")

		driver.SelectScreen(screenPin)
		driver.AssertScreen(screenPin)
		driver.Snapshot("02_pin_screen")

		driver.SelectScreen(screenConfig)
		driver.AssertScreen(screenConfig)
		driver.Snapshot("03_config_screen")

		driver.SelectScreen(screenLogs)
		driver.AssertScreen(screenLogs)
		driver.AssertViewContains("Logs", "Debug Log (tail)", "Loop Output (tail)")

		driver.SelectScreen(screenHelp)
		driver.AssertScreen(screenHelp)
		driver.AssertViewContains("Help", "Ctrl+F")
	})
}
