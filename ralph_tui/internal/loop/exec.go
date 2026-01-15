// Package loop provides command execution helpers.
// Entrypoint: RunCommand.
package loop

import (
	"os"
	"os/exec"

	"github.com/mitchfultz/ralph/ralph_tui/internal/procgroup"
)

// RunCommand executes a command and streams output to the logger.
func RunCommand(cmd *exec.Cmd, redactor *Redactor, logger Logger) error {
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
func RunCommandWithFile(cmd *exec.Cmd, redactor *Redactor, logger Logger, outputPath string) error {
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
