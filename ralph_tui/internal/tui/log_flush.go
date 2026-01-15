// Package tui provides shared viewport flush helpers.
package tui

import "time"

const logViewportFlushMaxLatency = 200 * time.Millisecond

func shouldFlushLogViewport(pending int, threshold int, lastFlush time.Time) bool {
	if pending <= 0 {
		return false
	}
	if pending >= threshold {
		return true
	}
	if lastFlush.IsZero() {
		return true
	}
	return time.Since(lastFlush) >= logViewportFlushMaxLatency
}
