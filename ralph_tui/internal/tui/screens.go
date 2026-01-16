// Package tui defines Ralph screen identifiers and navigation items.
// Entrypoint: navigationItems.
package tui

type screen int

const (
	screenDashboard screen = iota
	screenRunLoop
	screenBuildSpecs
	screenPin
	screenConfig
	screenLogs
	screenHelp
)

type navItem struct {
	title  string
	desc   string
	screen screen
}

func (n navItem) Title() string       { return n.title }
func (n navItem) Description() string { return n.desc }
func (n navItem) FilterValue() string { return n.title }
func (n navItem) navKey() string      { return "screen:" + screenName(n.screen) }

func navigationItems() []navItem {
	return []navItem{
		{title: "Dashboard", desc: "Overview of Ralph activity", screen: screenDashboard},
		{title: "Run Loop", desc: "Run loop controls", screen: screenRunLoop},
		{title: "Build Specs", desc: "Specs builder", screen: screenBuildSpecs},
		{title: "Pin", desc: "Pin manager", screen: screenPin},
		{title: "Config", desc: "Configuration editor", screen: screenConfig},
		{title: "Logs", desc: "Recent logs", screen: screenLogs},
		{title: "Help", desc: "Help and shortcuts", screen: screenHelp},
	}
}

func screenName(value screen) string {
	switch value {
	case screenDashboard:
		return "dashboard"
	case screenRunLoop:
		return "runloop"
	case screenBuildSpecs:
		return "buildspecs"
	case screenPin:
		return "pin"
	case screenConfig:
		return "config"
	case screenLogs:
		return "logs"
	case screenHelp:
		return "help"
	default:
		return "unknown"
	}
}
