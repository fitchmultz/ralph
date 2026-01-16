// Package pin provides helpers for default prompt templates.
package pin

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/prompts"
)

// EnsureSpecsTemplate ensures the specs templates exist and returns the active template path.
func EnsureSpecsTemplate(pinDir string, projectType project.Type) (string, error) {
	cleanPinDir := filepath.Clean(strings.TrimSpace(pinDir))
	if cleanPinDir == "" {
		return "", fmt.Errorf("pin directory is required")
	}
	resolvedType, err := project.ResolveType(projectType)
	if err != nil {
		return "", fmt.Errorf("project_type must be code or docs")
	}
	variants, err := specsTemplatesAll(cleanPinDir)
	if err != nil {
		return "", err
	}
	for _, variant := range variants {
		info, err := os.Stat(variant.path)
		if err == nil {
			if info.IsDir() {
				return "", fmt.Errorf("expected file at %s but found directory", variant.path)
			}
			continue
		}
		if !os.IsNotExist(err) {
			return "", err
		}
		if err := writeContent(variant.path, variant.content); err != nil {
			return "", err
		}
	}
	files := ResolveFiles(cleanPinDir)
	specsPath, err := files.RequiredSpecsBuilderPath(resolvedType)
	if err != nil {
		return "", err
	}
	return specsPath, nil
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
	content, err := prompts.PinSpecsBuilder(resolvedType)
	if err != nil {
		return "", "", err
	}
	files := ResolveFiles(cleanPinDir)
	specsPath, err := files.RequiredSpecsBuilderPath(resolvedType)
	if err != nil {
		return "", "", err
	}
	return specsPath, content, nil
}

type specsTemplateVariant struct {
	projectType project.Type
	path        string
	content     string
}

func specsTemplatesAll(pinDir string) ([]specsTemplateVariant, error) {
	variants := make([]specsTemplateVariant, 0, 2)
	for _, projectType := range []project.Type{project.TypeCode, project.TypeDocs} {
		path, content, err := specsTemplateForProject(pinDir, projectType)
		if err != nil {
			return nil, err
		}
		variants = append(variants, specsTemplateVariant{
			projectType: projectType,
			path:        path,
			content:     content,
		})
	}
	return variants, nil
}
