## What's Changed

{{CHANGELOG_SECTION}}

## Binaries

| Platform | Architecture | Binary |
|----------|-------------|--------|
| Linux | x86_64 | `ralph-{{VERSION}}-linux-x64.tar.gz` |
| macOS | x86_64 | `ralph-{{VERSION}}-macos-x64.tar.gz` |
| macOS | arm64 | `ralph-{{VERSION}}-macos-arm64.tar.gz` |

## Checksums

```
{{CHECKSUMS}}
```

## Installation

### Quick Install (macOS/Linux)

Download and extract the appropriate binary for your platform:

```bash
# macOS ARM64 (Apple Silicon)
curl -LO https://github.com/mitchfultz/ralph/releases/download/v{{VERSION}}/ralph-{{VERSION}}-macos-arm64.tar.gz
tar -xzf ralph-{{VERSION}}-macos-arm64.tar.gz
mv ralph ~/.local/bin/

# macOS x86_64 (Intel)
curl -LO https://github.com/mitchfultz/ralph/releases/download/v{{VERSION}}/ralph-{{VERSION}}-macos-x64.tar.gz
tar -xzf ralph-{{VERSION}}-macos-x64.tar.gz
mv ralph ~/.local/bin/

# Linux x86_64
curl -LO https://github.com/mitchfultz/ralph/releases/download/v{{VERSION}}/ralph-{{VERSION}}-linux-x64.tar.gz
tar -xzf ralph-{{VERSION}}-linux-x64.tar.gz
mv ralph ~/.local/bin/
```

### Verify Checksum

Before using the binary, verify its integrity:

```bash
shasum -a 256 -c ralph-{{VERSION}}-<platform>.tar.gz.sha256
```

### Build from Source

Alternatively, build from source:

```bash
git clone https://github.com/mitchfultz/ralph.git
cd ralph
git checkout v{{VERSION}}
make install
```

## Full Changelog

See [CHANGELOG.md](https://github.com/mitchfultz/ralph/blob/v{{VERSION}}/CHANGELOG.md) for the complete list of changes.
