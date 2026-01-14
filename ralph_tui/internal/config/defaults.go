// Package config embeds and decodes the Ralph default configuration.
// Entrypoint: DefaultConfig.
package config

import (
	"embed"
	"encoding/json"
	"strings"
)

//go:embed defaults.json
var defaultsFS embed.FS

// DefaultConfig returns the built-in base configuration.
func DefaultConfig() (Config, error) {
	data, err := defaultsFS.ReadFile("defaults.json")
	if err != nil {
		return Config{}, err
	}

	var cfg Config
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&cfg); err != nil {
		return Config{}, err
	}

	return cfg, nil
}
