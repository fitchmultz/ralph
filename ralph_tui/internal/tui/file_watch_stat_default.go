//go:build !linux && !darwin

// Package tui provides fallback file stat helpers for file change detection.
package tui

import (
	"os"
	"time"
)

type fileStatDetails struct {
	inode    uint64
	ctime    time.Time
	hasInode bool
	hasCtime bool
}

func readFileStatDetails(info os.FileInfo) (fileStatDetails, bool) {
	return fileStatDetails{}, false
}
