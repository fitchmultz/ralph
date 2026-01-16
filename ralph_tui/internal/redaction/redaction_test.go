package redaction

import "testing"

func TestLooksSensitiveEnvKey(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		key  string
		want bool
	}{
		{name: "api_key", key: "API_KEY", want: true},
		{name: "password", key: "PASSWORD", want: true},
		{name: "token_suffix", key: "auth-token", want: true},
		{name: "token_digits", key: "TOKEN1", want: true},
		{name: "trimmed", key: "  secret  ", want: true},
		{name: "path", key: "PATH", want: false},
		{name: "home", key: "HOME", want: false},
		{name: "shell", key: "SHELL", want: false},
		{name: "word_contains_key", key: "MONKEY", want: false},
		{name: "private_key", key: "PRIVATEKEY", want: false},
	}

	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			got := LooksSensitiveEnvKey(test.key)
			if got != test.want {
				t.Fatalf("LooksSensitiveEnvKey(%q) = %t; want %t", test.key, got, test.want)
			}
		})
	}
}

func TestIsPathLikeEnvKey(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		key  string
		want bool
	}{
		{name: "path", key: "PATH", want: true},
		{name: "home", key: "HOME", want: true},
		{name: "tmpdir", key: "TMPDIR", want: true},
		{name: "trimmed", key: "  pwd  ", want: true},
		{name: "shell", key: "SHELL", want: false},
		{name: "path_info", key: "PATH_INFO", want: false},
	}

	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			got := IsPathLikeEnvKey(test.key)
			if got != test.want {
				t.Fatalf("IsPathLikeEnvKey(%q) = %t; want %t", test.key, got, test.want)
			}
		})
	}
}
