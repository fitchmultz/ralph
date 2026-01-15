// Package loop provides command execution helpers.
// Entrypoint: RunCommand.
package loop

import (
	"context"
	"os"
	"os/exec"

	"github.com/mitchfultz/ralph/ralph_tui/internal/procgroup"
)

// RunCommand executes a command and streams output to the logger.
func RunCommand(ctx context.Context, cmd *exec.Cmd, redactor *Redactor, logger Logger) error {
	cmd = ensureCommandContext(ctx, cmd)
	procgroup.Configure(cmd)
	writer := newLineWriter(redactor, logger, nil)
	cmd.Stdout = writer
	cmd.Stderr = writer
	if cmd.Stdin == nil {
		cmd.Stdin = os.Stdin
	}
	err := cmd.Run()
	writer.Flush()
	return err
}

// RunCommandWithFile executes a command and streams output to logger and file.
func RunCommandWithFile(ctx context.Context, cmd *exec.Cmd, redactor *Redactor, logger Logger, outputPath string) error {
	cmd = ensureCommandContext(ctx, cmd)
	procgroup.Configure(cmd)
	file, err := os.Create(outputPath)
	if err != nil {
		return err
	}
	defer file.Close()

	writer := newLineWriter(redactor, logger, file)
	cmd.Stdout = writer
	cmd.Stderr = writer
	if cmd.Stdin == nil {
		cmd.Stdin = os.Stdin
	}
	err = cmd.Run()
	writer.Flush()
	return err
}

func ensureCommandContext(ctx context.Context, cmd *exec.Cmd) *exec.Cmd {
	if cmd == nil || cmd.Cancel != nil {
		return cmd
	}
	if ctx == nil {
		ctx = context.Background()
	}
	name := cmd.Path
	if name == "" && len(cmd.Args) > 0 {
		name = cmd.Args[0]
	}
	args := []string{}
	if len(cmd.Args) > 1 {
		args = append(args, cmd.Args[1:]...)
	}
	newCmd := exec.CommandContext(ctx, name, args...)
	newCmd.Env = cmd.Env
	newCmd.Dir = cmd.Dir
	newCmd.Stdin = cmd.Stdin
	newCmd.Stdout = cmd.Stdout
	newCmd.Stderr = cmd.Stderr
	newCmd.ExtraFiles = cmd.ExtraFiles
	newCmd.SysProcAttr = cmd.SysProcAttr
	return newCmd
}
