// Package loop provides runner invocation for the Ralph loop.
// Entrypoint: RunnerInvoker.
package loop

import (
	"context"
	"fmt"
	"os"
	"os/exec"
)

// RunnerInvoker invokes Codex or opencode.
type RunnerInvoker struct {
	Runner     string
	RunnerArgs []string
	Redactor   *Redactor
	Logger     Logger
}

// RunPrompt runs the runner using the provided prompt file.
func (r RunnerInvoker) RunPrompt(ctx context.Context, promptPath string) error {
	switch r.Runner {
	case "codex":
		args := append([]string{"exec"}, r.RunnerArgs...)
		args = append(args, "-")
		cmd := exec.CommandContext(ctx, "codex", args...)
		file, err := os.Open(promptPath)
		if err != nil {
			return err
		}
		defer file.Close()
		cmd.Stdin = file
		if err := RunCommand(ctx, cmd, r.Redactor, r.Logger); err != nil {
			return fmt.Errorf("codex failed while running loop: %w", err)
		}
		return nil
	case "opencode":
		args := append([]string{"run"}, r.RunnerArgs...)
		args = append(args, "--file", promptPath, "--", "Follow the attached prompt file verbatim.")
		cmd := exec.CommandContext(ctx, "opencode", args...)
		if err := RunCommand(ctx, cmd, r.Redactor, r.Logger); err != nil {
			return fmt.Errorf("opencode failed while running loop: %w", err)
		}
		return nil
	default:
		return fmt.Errorf("--runner must be codex or opencode (got: %s)", r.Runner)
	}
}
