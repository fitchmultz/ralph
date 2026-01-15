// Package tui provides lightweight file change helpers for periodic refresh.
package tui

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"strings"
	"time"
)

type fileStamp struct {
	Exists   bool
	ModTime  time.Time
	Size     int64
	Inode    uint64
	HasInode bool
	Ctime    time.Time
	HasCtime bool
	Hash     string
	HasHash  bool
}

const fileStampHashMaxBytes int64 = 64 * 1024

func getFileStamp(path string) (fileStamp, error) {
	info, err := os.Stat(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fileStamp{Exists: false}, nil
		}
		return fileStamp{}, err
	}
	stamp := fileStamp{
		Exists:  true,
		ModTime: info.ModTime(),
		Size:    info.Size(),
	}
	if details, ok := readFileStatDetails(info); ok {
		stamp.Inode = details.inode
		stamp.HasInode = details.hasInode
		stamp.Ctime = details.ctime
		stamp.HasCtime = details.hasCtime
	}
	if info.Size() <= fileStampHashMaxBytes {
		hash, err := hashFileContents(path)
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				return fileStamp{Exists: false}, nil
			}
			return fileStamp{}, err
		}
		stamp.Hash = hash
		stamp.HasHash = true
	}
	return stamp, nil
}

func fileChanged(path string, last fileStamp) (fileStamp, bool, error) {
	stamp, err := getFileStamp(path)
	if err != nil {
		return fileStamp{}, false, err
	}
	return stamp, !sameFileStamp(stamp, last), nil
}

func sameFileStamp(left fileStamp, right fileStamp) bool {
	if left.Exists != right.Exists {
		return false
	}
	if !left.Exists {
		return true
	}
	if left.Size != right.Size {
		return false
	}
	if !left.ModTime.Equal(right.ModTime) {
		return false
	}
	if left.HasInode && right.HasInode && left.Inode != right.Inode {
		return false
	}
	if left.HasCtime && right.HasCtime && !left.Ctime.Equal(right.Ctime) {
		return false
	}
	if left.HasHash && right.HasHash && left.Hash != right.Hash {
		return false
	}
	return true
}

func fileStampSignature(stamp fileStamp) string {
	if !stamp.Exists {
		return "missing"
	}
	parts := []string{
		fmt.Sprintf("size=%d", stamp.Size),
		fmt.Sprintf("mtime=%d", stamp.ModTime.UnixNano()),
	}
	if stamp.HasInode {
		parts = append(parts, fmt.Sprintf("inode=%d", stamp.Inode))
	}
	if stamp.HasCtime {
		parts = append(parts, fmt.Sprintf("ctime=%d", stamp.Ctime.UnixNano()))
	}
	if stamp.HasHash {
		parts = append(parts, "hash="+stamp.Hash)
	}
	return strings.Join(parts, ";")
}

func hashFileContents(path string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:]), nil
}
