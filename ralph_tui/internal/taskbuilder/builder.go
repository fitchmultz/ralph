// Package taskbuilder builds queue items from prompt input.
package taskbuilder

import (
	"context"
	"fmt"
	"strings"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

// BuildOptions controls task builder behavior.
type BuildOptions struct {
	RepoRoot    string
	PinDir      string
	ProjectType project.Type
	Prompt      string

	Tags        []string
	Scope       string
	Description string

	WriteToQueue bool
	InsertAtTop  bool
}

// BuildResult captures queue item output.
type BuildResult struct {
	ID        string
	ItemBlock []string
}

// Build constructs a valid queue item and optionally writes it into Queue.
func Build(ctx context.Context, opts BuildOptions) (BuildResult, error) {
	if strings.TrimSpace(opts.RepoRoot) == "" {
		return BuildResult{}, fmt.Errorf("repo root required")
	}
	if strings.TrimSpace(opts.PinDir) == "" {
		return BuildResult{}, fmt.Errorf("pin dir required")
	}
	prompt := strings.TrimSpace(opts.Prompt)
	if prompt == "" {
		return BuildResult{}, fmt.Errorf("prompt text required")
	}

	projectType, err := project.ResolveType(opts.ProjectType)
	if err != nil {
		return BuildResult{}, err
	}

	tags := opts.Tags
	if len(tags) == 0 {
		tags = defaultTagsFor(projectType)
	}
	tags, err = pin.ValidateTagList("task builder tags", strings.Join(tags, " "))
	if err != nil {
		return BuildResult{}, err
	}

	description := strings.TrimSpace(opts.Description)
	if description == "" {
		description = deriveDescription(prompt)
	}
	if description == "" {
		return BuildResult{}, fmt.Errorf("description required")
	}
	if ctx != nil {
		select {
		case <-ctx.Done():
			return BuildResult{}, ctx.Err()
		default:
		}
	}

	files := pin.ResolveFiles(opts.PinDir)
	id, err := pin.NextQueueID(files, "")
	if err != nil {
		return BuildResult{}, err
	}

	block, err := FormatQueueItemBlock(FormatOptions{
		ID:          id,
		Tags:        tags,
		Description: description,
		Scope:       opts.Scope,
		Prompt:      prompt,
	})
	if err != nil {
		return BuildResult{}, err
	}

	if opts.WriteToQueue {
		if err := pin.InsertQueueItem(files.QueuePath, block, pin.InsertQueueOptions{InsertAtTop: opts.InsertAtTop}); err != nil {
			return BuildResult{}, err
		}
	}

	return BuildResult{ID: id, ItemBlock: block}, nil
}

func defaultTagsFor(projectType project.Type) []string {
	if projectType == project.TypeDocs {
		return []string{"docs"}
	}
	return []string{"code"}
}
