// Package config writes configuration layers to disk with deterministic formatting.
// Entrypoint: SavePartial.
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/fileutil"
)

// SaveOptions controls how config is written.
type SaveOptions struct {
	// RelativeRoot indicates when to convert absolute paths into relative ones.
	RelativeRoot string
}

// SavePartial persists a partial configuration to disk, creating directories as needed.
func SavePartial(path string, partial PartialConfig, opts SaveOptions) error {
	prepared := preparePartialForSave(partial, opts.RelativeRoot)

	payload, err := json.MarshalIndent(prepared, "", "  ")
	if err != nil {
		return err
	}

	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}

	return fileutil.WriteFileAtomic(path, payload, 0o600)
}

func preparePartialForSave(partial PartialConfig, relativeRoot string) PartialConfig {
	if relativeRoot == "" {
		return partial
	}

	relativeRoot = filepath.Clean(relativeRoot)

	if partial.Paths != nil {
		if partial.Paths.DataDir != nil {
			value := maybeRelativizePath(*partial.Paths.DataDir, relativeRoot)
			partial.Paths.DataDir = &value
		}
		if partial.Paths.CacheDir != nil {
			value := maybeRelativizePath(*partial.Paths.CacheDir, relativeRoot)
			partial.Paths.CacheDir = &value
		}
		if partial.Paths.PinDir != nil {
			value := maybeRelativizePath(*partial.Paths.PinDir, relativeRoot)
			partial.Paths.PinDir = &value
		}
	}
	if partial.Logging != nil {
		if partial.Logging.File != nil {
			value := maybeRelativizePath(*partial.Logging.File, relativeRoot)
			partial.Logging.File = &value
		}
	}

	return partial
}

func maybeRelativizePath(pathValue string, root string) string {
	clean := filepath.Clean(strings.TrimSpace(pathValue))
	if clean == "" || !filepath.IsAbs(clean) {
		return pathValue
	}
	rel, err := filepath.Rel(root, clean)
	if err != nil {
		return pathValue
	}
	if rel == "." || strings.HasPrefix(rel, "..") {
		return pathValue
	}
	return rel
}
