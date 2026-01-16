// Package pin provides helpers for default prompt templates.
package pin

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

// EnsureSpecsTemplate ensures the project-specific specs template exists and returns its path.
func EnsureSpecsTemplate(pinDir string, projectType project.Type) (string, error) {
	path, content, err := specsTemplateForProject(pinDir, projectType)
	if err != nil {
		return "", err
	}
	info, err := os.Stat(path)
	if err == nil {
		if info.IsDir() {
			return "", fmt.Errorf("expected file at %s but found directory", path)
		}
		return path, nil
	}
	if !os.IsNotExist(err) {
		return "", err
	}
	if err := writeContent(path, content); err != nil {
		return "", err
	}
	return path, nil
}

func specsTemplateForProject(pinDir string, projectType project.Type) (string, string, error) {
	cleanPinDir := filepath.Clean(strings.TrimSpace(pinDir))
	if cleanPinDir == "" {
		return "", "", fmt.Errorf("pin directory is required")
	}
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", "", fmt.Errorf("project_type must be code or docs")
	}
	if resolvedType == project.TypeDocs {
		return filepath.Join(cleanPinDir, "specs_builder_docs.md"), defaultSpecsBuilderDocsContent, nil
	}
	return filepath.Join(cleanPinDir, "specs_builder.md"), defaultSpecsBuilderContent, nil
}
