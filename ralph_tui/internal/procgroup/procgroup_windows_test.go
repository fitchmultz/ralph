//go:build windows

package procgroup

import (
	"os/exec"
	"testing"
)

func TestConfigureNoop(t *testing.T) {
	cmd := exec.Command("cmd", "/C", "echo", "ready")
	Configure(cmd)

	if cmd.SysProcAttr != nil {
		t.Fatalf("expected SysProcAttr to remain nil on windows")
	}
	if cmd.Cancel != nil {
		t.Fatalf("expected Cancel handler to remain nil on windows")
	}
}

func TestConfigureNoopWithNilCommand(t *testing.T) {
	Configure(nil)
}
