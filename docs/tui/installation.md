# Installation

## From crates.io

```bash
cargo install roam-sdk
```

This installs the `roam` binary.

## From source

```bash
git clone https://github.com/avelino/roam-tui.git
cd roam-tui
cargo install --path .
```

## From GitHub releases

Pre-built binaries are available for every release:

| Platform | Target |
|---|---|
| Linux x86_64 | `roam-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `roam-aarch64-unknown-linux-gnu` |
| macOS x86_64 | `roam-x86_64-apple-darwin` |
| macOS ARM64 (Apple Silicon) | `roam-aarch64-apple-darwin` |
| Windows x86_64 | `roam-x86_64-pc-windows-msvc.exe` |

Download from [Releases](https://github.com/avelino/roam-tui/releases), make executable (`chmod +x roam-*`), and move to your PATH.

## First run

On first run, `roam` creates a default config file and tells you where it is:

```
Created default config at: ~/.config/roam-tui/config.toml
Please edit it with your Roam graph name and API token, then run again.
```

Edit the file with your graph name and API token, then run `roam` again.

## Getting an API token

1. Open your Roam graph in the browser
2. Go to **Settings** → **Graph** → **API tokens**
3. Create a new token with read/write permissions
4. Copy the token into your config file or set the `ROAM_GRAPH_API__TOKEN` environment variable
