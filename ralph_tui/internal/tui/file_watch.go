// Package tui provides lightweight file change helpers for periodic refresh.
package tui

import (
	"errors"
	"os"
	"time"
)

func fileModTime(path string) (time.Time, error) {
	info, err := os.Stat(path)
	if err != nil {
		return time.Time{}, err
	}
	return info.ModTime(), nil
}

func fileChanged(path string, last time.Time) (time.Time, bool, error) {
	modTime, err := fileModTime(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			if last.IsZero() {
				return time.Time{}, false, nil
			}
			return time.Time{}, true, nil
		}
		return time.Time{}, false, err
	}
	return modTime, modTime.After(last), nil
}
