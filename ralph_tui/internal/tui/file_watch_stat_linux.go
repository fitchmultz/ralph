//go:build linux

// Package tui provides Linux-specific file stat helpers for file change detection.
package tui

import (
	"os"
	"syscall"
	"time"
)

type fileStatDetails struct {
	inode    uint64
	ctime    time.Time
	hasInode bool
	hasCtime bool
}

func readFileStatDetails(info os.FileInfo) (fileStatDetails, bool) {
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || stat == nil {
		return fileStatDetails{}, false
	}
	return fileStatDetails{
		inode:    stat.Ino,
		hasInode: true,
		ctime:    time.Unix(int64(stat.Ctim.Sec), int64(stat.Ctim.Nsec)),
		hasCtime: true,
	}, true
}
