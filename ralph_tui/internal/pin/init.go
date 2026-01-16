// Package pin initializes Ralph pin and cache layouts.
// Entrypoint: InitLayout.
package pin

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/fileutil"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

// InitOptions configures how pin initialization behaves.
type InitOptions struct {
	Force       bool
	ProjectType project.Type
}

// InitResult captures initialization outcomes.
type InitResult struct {
	PinDir      string
	CacheDir    string
	Created     []string
	Overwritten []string
	Skipped     []string
}

// InitLayout ensures the pin and cache directories exist and seeds missing pin files.
func InitLayout(pinDir string, cacheDir string, opts InitOptions) (InitResult, error) {
	cleanPinDir := filepath.Clean(strings.TrimSpace(pinDir))
	cleanCacheDir := filepath.Clean(strings.TrimSpace(cacheDir))
	if cleanPinDir == "" {
		return InitResult{}, fmt.Errorf("pin directory is required")
	}
	if cleanCacheDir == "" {
		return InitResult{}, fmt.Errorf("cache directory is required")
	}

	if err := os.MkdirAll(cleanPinDir, 0o755); err != nil {
		return InitResult{}, err
	}
	if err := os.MkdirAll(cleanCacheDir, 0o755); err != nil {
		return InitResult{}, err
	}

	lock, err := acquirePinLock(cleanPinDir)
	if err != nil {
		return InitResult{}, err
	}
	defer lock.Release()

	files := ResolveFiles(cleanPinDir)
	entries := []struct {
		path    string
		content string
	}{
		{path: files.QueuePath, content: defaultQueueContent},
		{path: files.DonePath, content: defaultDoneContent},
		{path: files.LookupPath, content: defaultLookupContent},
		{path: files.ReadmePath, content: defaultReadmeContent},
		{path: files.SpecsPath, content: defaultSpecsBuilderContent},
	}

	projectType, err := project.ResolveType(opts.ProjectType)
	if err != nil {
		return InitResult{}, fmt.Errorf("project_type must be code or docs")
	}
	if projectType == project.TypeDocs {
		docsTemplate := filepath.Join(cleanPinDir, "specs_builder_docs.md")
		entries = append(entries, struct {
			path    string
			content string
		}{path: docsTemplate, content: defaultSpecsBuilderDocsContent})
	}

	result := InitResult{
		PinDir:   cleanPinDir,
		CacheDir: cleanCacheDir,
	}

	for _, entry := range entries {
		status, err := writeInitFile(entry.path, entry.content, opts.Force)
		if err != nil {
			return result, err
		}
		switch status {
		case initCreated:
			result.Created = append(result.Created, filepath.Base(entry.path))
		case initOverwritten:
			result.Overwritten = append(result.Overwritten, filepath.Base(entry.path))
		case initSkipped:
			result.Skipped = append(result.Skipped, filepath.Base(entry.path))
		}
	}

	sort.Strings(result.Created)
	sort.Strings(result.Overwritten)
	sort.Strings(result.Skipped)

	return result, nil
}

// MissingFiles returns missing pin files (full paths).
func MissingFiles(files Files) []string {
	paths := []string{
		files.QueuePath,
		files.DonePath,
		files.LookupPath,
		files.ReadmePath,
		files.SpecsPath,
	}
	missing := make([]string, 0)
	for _, path := range paths {
		if path == "" {
			continue
		}
		if ok, err := fileIsMissing(path); err == nil && ok {
			missing = append(missing, path)
		}
	}
	sort.Strings(missing)
	return missing
}

type initStatus int

const (
	initCreated initStatus = iota
	initOverwritten
	initSkipped
)

func writeInitFile(path string, content string, force bool) (initStatus, error) {
	info, err := os.Stat(path)
	if err == nil {
		if info.IsDir() {
			return initSkipped, fmt.Errorf("expected file at %s but found directory", path)
		}
		if !force {
			return initSkipped, nil
		}
		if err := writeContent(path, content); err != nil {
			return initSkipped, err
		}
		return initOverwritten, nil
	}
	if !os.IsNotExist(err) {
		return initSkipped, err
	}
	if err := writeContent(path, content); err != nil {
		return initSkipped, err
	}
	return initCreated, nil
}

func writeContent(path string, content string) error {
	payload := content
	if !strings.HasSuffix(payload, "\n") {
		payload += "\n"
	}
	return fileutil.WriteFileAtomic(path, []byte(payload), 0o600)
}

func fileIsMissing(path string) (bool, error) {
	info, err := os.Stat(path)
	if err == nil {
		return info.IsDir(), nil
	}
	if os.IsNotExist(err) {
		return true, nil
	}
	return false, err
}
