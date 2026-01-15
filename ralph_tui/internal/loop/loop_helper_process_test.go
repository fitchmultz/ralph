//go:build !windows

// Package loop provides helper process entrypoints for cancellation tests.
package loop

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

func TestMain(m *testing.M) {
	if os.Getenv("RALPH_HELPER_PROCESS") == "1" {
		helperMain()
		os.Exit(0)
	}
	os.Exit(m.Run())
}

func helperMain() {
	mode := os.Getenv("RALPH_HELPER_MODE")
	queuePath := os.Getenv("RALPH_QUEUE_PATH")
	itemID := os.Getenv("RALPH_ITEM_ID")
	repoRoot := os.Getenv("RALPH_REPO_ROOT")

	if queuePath == "" || itemID == "" {
		_, _ = fmt.Fprintln(os.Stderr, "missing queue path or item ID")
		os.Exit(2)
	}

	switch mode {
	case "runner_cancel":
		markQueueChecked(queuePath, itemID)
		fmt.Println("RUNNER_MARKED")
		time.Sleep(30 * time.Second)
	case "runner_complete_specs_only":
		markQueueChecked(queuePath, itemID)
		fmt.Println("RUNNER_DONE")
	case "runner_complete_with_code_change":
		if repoRoot == "" {
			_, _ = fmt.Fprintln(os.Stderr, "missing repo root")
			os.Exit(2)
		}
		markQueueChecked(queuePath, itemID)
		touchReadme(repoRoot)
		fmt.Println("RUNNER_DONE")
	default:
		_, _ = fmt.Fprintf(os.Stderr, "unknown helper mode: %s\n", mode)
		os.Exit(2)
	}
}

func markQueueChecked(queuePath string, itemID string) {
	updated, _, err := pin.ToggleQueueItemChecked(queuePath, itemID)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "toggle failed: %v\n", err)
		os.Exit(2)
	}
	if !updated {
		_, _ = fmt.Fprintf(os.Stderr, "item %s not found in queue\n", itemID)
		os.Exit(2)
	}
}

func touchReadme(repoRoot string) {
	readmePath := filepath.Join(repoRoot, "README.md")
	file, err := os.OpenFile(readmePath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "open README failed: %v\n", err)
		os.Exit(2)
	}
	defer file.Close()
	if _, err := file.WriteString("\nchange\n"); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "write README failed: %v\n", err)
		os.Exit(2)
	}
}
