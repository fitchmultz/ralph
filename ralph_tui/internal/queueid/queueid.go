// Package queueid provides exact queue item ID parsing helpers.
package queueid

import "regexp"

var idPattern = regexp.MustCompile(`[A-Z0-9]{2,10}-\d{4}`)

// Extract returns the first exact ID match in the line, ignoring partial suffixes.
func Extract(line string) string {
	matches := idPattern.FindAllStringIndex(line, -1)
	for _, match := range matches {
		end := match[1]
		if end < len(line) {
			next := line[end]
			if next >= '0' && next <= '9' {
				continue
			}
		}
		return line[match[0]:match[1]]
	}
	return ""
}
