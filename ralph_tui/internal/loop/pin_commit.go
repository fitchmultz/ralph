// Package loop provides git helpers for the Ralph loop.
// Entrypoint: CommitPinChanges, AutoCommitPinOnlyChanges.
package loop

import (
	"context"
	"fmt"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

// CommitPinChanges commits only pin file changes, refusing to include staged non-pin changes.
func CommitPinChanges(ctx context.Context, repoRoot string, files pin.Files, message string) (bool, error) {
	status, err := StatusDetails(ctx, repoRoot)
	if err != nil {
		return false, err
	}
	pinSet := files.RelativePathSet(repoRoot)
	if hasAnyStagedTrackedChangesOutsidePin(status, pinSet) {
		return false, fmt.Errorf("staged non-pin changes detected; commit pin-only changes requires a clean index")
	}
	if !hasAnyPinTrackedChanges(status, pinSet) {
		return false, nil
	}
	if err := CommitPaths(ctx, repoRoot, message, files.AllPaths()...); err != nil {
		return false, err
	}
	return true, nil
}

// AutoCommitPinOnlyChanges commits pin changes only when the repo dirtiness is strictly pin-only.
func AutoCommitPinOnlyChanges(ctx context.Context, repoRoot string, files pin.Files, message string) (bool, error) {
	status, err := StatusDetails(ctx, repoRoot)
	if err != nil {
		return false, err
	}
	pinSet := files.RelativePathSet(repoRoot)
	if !hasAnyPinTrackedChanges(status, pinSet) {
		return false, nil
	}
	if hasAnyTrackedChangesOutsidePin(status, pinSet) {
		return false, nil
	}
	if hasAnyUntracked(status) {
		return false, nil
	}
	return CommitPinChanges(ctx, repoRoot, files, message)
}

func hasAnyPinTrackedChanges(status GitStatus, pinSet map[string]struct{}) bool {
	for _, entry := range status.Entries {
		if entry.IsTracked() && isEntryInPinSet(entry, pinSet) {
			return true
		}
	}
	return false
}

func hasAnyTrackedChangesOutsidePin(status GitStatus, pinSet map[string]struct{}) bool {
	for _, entry := range status.Entries {
		if entry.IsTracked() && !isEntryInPinSet(entry, pinSet) {
			return true
		}
	}
	return false
}

func hasAnyStagedTrackedChangesOutsidePin(status GitStatus, pinSet map[string]struct{}) bool {
	for _, entry := range status.Entries {
		if entry.IsTracked() && entry.XY != "" && entry.XY != "??" && entry.XY[0] != ' ' && !isEntryInPinSet(entry, pinSet) {
			return true
		}
	}
	return false
}

func hasAnyUntracked(status GitStatus) bool {
	for _, entry := range status.Entries {
		if entry.IsUntracked() {
			return true
		}
	}
	return false
}

func isEntryInPinSet(entry GitStatusEntry, pinSet map[string]struct{}) bool {
	if entry.Path == "" {
		return false
	}
	if _, ok := pinSet[entry.Path]; !ok {
		return false
	}
	if entry.OrigPath != "" {
		if _, ok := pinSet[entry.OrigPath]; !ok {
			return false
		}
	}
	return true
}
