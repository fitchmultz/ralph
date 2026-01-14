// Package loop provides tests for queue parsing.
// Entrypoint: go test ./...
package loop

import (
	"os"
	"path/filepath"
	"testing"
)

func TestFirstUncheckedItemWithTags(t *testing.T) {
	content := "## Queue\n" +
		"- [ ] RQ-0001 [db]: First item. (a)\n" +
		"- [ ] RQ-0002 [ui]: Second item. (b)\n" +
		"\n## Blocked\n\n## Parking Lot\n"
	path := filepath.Join(t.TempDir(), "queue.md")
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}

	item, err := FirstUncheckedItem(path, []string{"ui"})
	if err != nil {
		t.Fatalf("FirstUncheckedItem failed: %v", err)
	}
	if item == nil || item.ID != "RQ-0002" {
		t.Fatalf("expected RQ-0002, got %#v", item)
	}
}
