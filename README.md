# rtop

htop, but with live time-series charts, color-graded gauges, GPU/NPU tracking,
and a Docker container panel with filtering.

- **Overview** — per-core CPU gauges, system totals (CPU/MEM/SWAP/GPU/NPU),
  and a focused time-series chart (cycle through CPU, memory, network, disk,
  GPU, NPU with `←`/`→`) backed by up to 3 minutes of history.
- **Processes** — sortable, filterable, htop-style table with a confirm-gated
  kill action.
- **Docker** — live per-container CPU/mem/net stats straight from the Docker
  Engine API, filterable, with a confirm-gated stop action.
- **GPU/NPU** — on macOS, GPU usage via IOKit (no sudo) and Apple Neural
  Engine activity via IOReport (no sudo); on Linux, NVIDIA via NVML and AMD
  via sysfs.

## Install

### Homebrew (macOS and Linux)

```sh
brew tap regisrex/rtop
brew install rtop
```

### apt (Debian/Ubuntu, x86_64 and arm64)

```sh
curl -fsSL https://regisrex.github.io/rtop/apt/gpg.key | sudo gpg --dearmor -o /usr/share/keyrings/rtop-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/rtop-archive-keyring.gpg] https://regisrex.github.io/rtop/apt stable main" | sudo tee /etc/apt/sources.list.d/rtop.list
sudo apt update
sudo apt install rtop
```

### From source

```sh
cargo install --git https://github.com/regisrex/rtop
```

## Usage

```sh
rtop                  # default 1s refresh
rtop --interval 2000  # refresh every 2s — lighter on CPU
```

| Key | Action |
|---|---|
| `q` / `Ctrl+C` | quit |
| `Tab` / `Shift+Tab` | cycle view |
| `1` `2` `3` | jump to view |
| `?` | help overlay |
| `←` `→` / `m` (Overview) | cycle the focused chart metric |
| `z` (Overview) | cycle the chart's time window (60s/120s/180s) |
| `/` (Processes/Docker) | filter |
| `c` `M` `p` `n` (Processes) | sort by cpu / mem / pid / name |
| `x` (Processes) | kill selected (confirm) |
| `a` (Docker) | toggle all / running-only |
| `s` (Docker) | stop selected (confirm) |

## Building

```sh
cargo build --release
./target/release/rtop
```

Requires Rust 2024 edition (rustc 1.85+). On macOS, GPU/NPU tracking links
against `IOKit`/`CoreFoundation`; on Linux, NVIDIA support loads
`libnvidia-ml.so` at runtime if present (no build-time dependency).

## License

MIT — see [LICENSE](LICENSE).
