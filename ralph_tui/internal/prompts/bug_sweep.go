package prompts

import "github.com/mitchfultz/ralph/ralph_tui/internal/project"

const (
	bugSweepCodePath = "defaults/specs_bug_sweep_code.md"
	bugSweepDocsPath = "defaults/specs_bug_sweep_docs.md"
)

// BugSweepEntry returns the default bug-sweep prompt entry for a project type.
func BugSweepEntry(projectType project.Type) (string, error) {
	resolved, err := project.ResolveType(projectType)
	if err != nil {
		return "", err
	}

	filename := bugSweepCodePath
	if resolved == project.TypeDocs {
		filename = bugSweepDocsPath
	}
	content, err := defaultPrompts.ReadFile(filename)
	if err != nil {
		return "", err
	}
	return string(content), nil
}
