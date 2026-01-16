// Package loop provides runner invocation for the Ralph loop.
// Entrypoint: RunnerInvoker.
package loop

import (
	"context"
	"fmt"
	"os"
	"os/exec"

	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
)

// RunnerInvoker invokes Codex or opencode.
type RunnerInvoker struct {
	Runner              string
	RunnerArgs          []string
	Redactor            *Redactor
	Logger              Logger
	LogMaxBufferedBytes int
	DisableStdin        bool
}

// RunPrompt runs the runner using the provided prompt file.
func (r RunnerInvoker) RunPrompt(ctx context.Context, promptPath string) error {
	invocation, err := runnerargs.BuildRunnerCommand(r.Runner, r.RunnerArgs, "", promptPath, false)
	if err != nil {
		return err
	}
	cmd := exec.CommandContext(ctx, invocation.Name, invocation.Args...)
	if invocation.PromptStdinPath != "" {
		file, err := os.Open(invocation.PromptStdinPath)
		if err != nil {
			return err
		}
		defer file.Close()
		cmd.Stdin = file
	}
	if r.DisableStdin && cmd.Stdin == nil {
		file, err := os.Open(os.DevNull)
		if err != nil {
			return err
		}
		defer file.Close()
		cmd.Stdin = file
	}
	if err := RunCommand(ctx, cmd, r.Redactor, r.Logger, r.LogMaxBufferedBytes); err != nil {
		return fmt.Errorf("%s failed while running loop: %w", invocation.Name, err)
	}
	return nil
}
