## What's Changed

{{CHANGELOG_SECTION}}

## Release Assets

This release attaches the artifacts built locally for the release host. Download the tarball that matches your platform from the assets list below.

## Checksums

```
{{CHECKSUMS}}
```

## Installation

### Quick Install (macOS/Linux)

Download the appropriate release asset for your platform, then extract and install it:

```bash
curl -LO {{REPO_URL}}/releases/download/v{{VERSION}}/ralph-{{VERSION}}-<platform>.tar.gz
tar -xzf ralph-{{VERSION}}-<platform>.tar.gz
mv ralph ~/.local/bin/
```

### Verify Checksum

Before using the binary, verify its integrity:

```bash
# macOS
shasum -a 256 -c ralph-{{VERSION}}-<platform>.tar.gz.sha256

# Linux
sha256sum -c ralph-{{VERSION}}-<platform>.tar.gz.sha256
```

### Build from Source

Alternatively, build from source:

```bash
git clone {{REPO_CLONE_URL}}
cd ralph
git checkout v{{VERSION}}
make install
```

## Full Changelog

See [CHANGELOG.md]({{REPO_URL}}/blob/v{{VERSION}}/CHANGELOG.md) for the complete list of changes.
