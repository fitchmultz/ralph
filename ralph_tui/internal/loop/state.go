// Package loop provides loop state updates for observers.
// Entrypoint: State, StateSink.
package loop

// Mode describes the loop runner mode.
type Mode string

const (
	ModeIdle       Mode = "idle"
	ModeOnce       Mode = "once"
	ModeContinuous Mode = "continuous"
)

// State describes the current loop status for UI consumers.
type State struct {
	Mode            Mode
	Iteration       int
	ActiveItemID    string
	ActiveItemTitle string
}

// StateSink receives loop state updates.
type StateSink interface {
	Update(State)
}
