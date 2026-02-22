# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Desktop App
```bash
pnpm tauri dev        # run the full app (starts Vite + Rust + sidecar)
pnpm tauri build      # production build
```

### Rust Workspace
```bash
cargo check -p spatia_engine          # check engine crate
cargo test --workspace                 # run all tests
cargo test -p spatia_engine            # run engine tests only
cargo clippy --workspace               # lint (must be zero warnings)
```

### CLI
```bash
cargo run -p spatia_cli -- ingest ./spatia.duckdb ./data/sample.csv places
cargo run -p spatia_cli -- schema ./spatia.duckdb places
cargo run -p spatia_cli -- geocode "San Francisco, CA"
echo "ingest ./spatia.duckdb ./data/sample.csv places" | cargo run -p spatia_cli
```

### Python Geocoder Sidecar
```bash
# Run server mode directly
python src-python/spatia-geocoder/main.py --serve

# Run one-shot mode
python src-python/spatia-geocoder/main.py "San Francisco, CA"

# Build sidecar binary
cd src-python/spatia-geocoder && pyinstaller --onefile main.py

# Install binary for Tauri bundling
bash src-python/spatia-geocoder/scripts/package_sidecar.sh
```

## Architecture

Spatia is a desktop GIS app with four layers:

1. **React/Vite frontend** (`src/`) — UI (TanStack Router + MapLibre GL + Deck.gl planned)
2. **Tauri host** (`src-tauri/src/`) — desktop runtime, Tauri command bridge, spawns the geocoder sidecar at startup
3. **Rust engine crate** (`src-tauri/crates/engine/`) — all domain logic: CSV ingestion, schema extraction, geocoding, command parsing
4. **Python geocoder sidecar** (`src-python/spatia-geocoder/`) — FastAPI + geopy, called from Rust via HTTP or subprocess

### Rust Workspace Layout

`src-tauri/Cargo.toml` defines the workspace with three members:
- `spatia` — Tauri app shell (thin, delegates to `spatia_engine`)
- `spatia_engine` — reusable domain logic; all business logic lives here
- `spatia_cli` — thin CLI wrapper that serializes argv into engine command strings

### Engine Modules (`spatia_engine`)

| Module | Purpose |
|---|---|
| `executor` | `execute_command(cmd: &str)` — parses and dispatches string commands (`ingest`, `schema`, `geocode`) |
| `ingest` | `ingest_csv` / `ingest_csv_to_table` — loads CSV into DuckDB via `read_csv_auto`; always loads spatial extension first |
| `schema` | `table_schema` / `raw_staging_schema` — queries `PRAGMA table_info` and returns `Vec<TableColumn>` |
| `geocode` | `geocode_batch_hybrid` — hybrid geocoding with auto/oneshot/daemon modes |
| `db_manager` | `DbManager` — thin wrapper holding a DuckDB `Connection` |
| `identifiers` | SQL identifier validation to prevent injection |
| `types` | `EngineResult<T>` — `Result<T, Box<dyn Error + Send + Sync>>` |

### String Command Interface

Both CLI and the Tauri `execute_engine_command` invoke handler share a single text-based command surface parsed by `executor.rs`:

```
ingest <db_path> <csv_path> [table_name]   → JSON {"status":"ok","table":"..."}
schema <db_path> <table_name>              → JSON array of TableColumn
geocode <address_1> <address_2> ...        → JSON array of GeocodeResult
```

Quoted arguments (single or double) are supported in the tokenizer.

### Geocoding Modes

Controlled by `SPATIA_GEOCODER_MODE` env var (`oneshot` | `daemon` | `auto`, default `auto`):
- **oneshot** — invokes the sidecar binary directly, reads JSON from stdout
- **daemon** — starts the sidecar with `--serve`, calls `POST /geocode` over `http://127.0.0.1:7788`
- **auto** — uses oneshot for batches below `SPATIA_GEOCODER_DAEMON_THRESHOLD` (default 100), daemon above; falls back to daemon if oneshot fails

Other geocoder env vars: `SPATIA_GEOCODER_BIN` (explicit binary path), `SPATIA_GEOCODER_PYTHON` (explicit interpreter), `SPATIA_GEOCODER_PORT` (daemon port), `SPATIA_GEOCODER_DEBUG` (verbose status/error fields).

### Tauri ↔ Engine Bridge

`src-tauri/src/lib.rs` exposes one invoke handler: `execute_engine_command(command: String) -> Result<String, String>` which forwards to `spatia_engine::execute_command`. The geocoder sidecar is also spawned at app startup via `tauri_plugin_shell`.

### Sidecar Bundling

The PyInstaller binary must be named `spatia-geocoder-<target-triple>` in `src-tauri/binaries/` for Tauri bundling. Use `package_sidecar.sh` after each Python build. The engine discovers the binary by checking several candidate paths relative to the working directory.

## Quality Gates

Always run before finalizing code:
```bash
cargo test --workspace
cargo clippy --workspace   # zero warnings required
```

## Key Constraints

- Spatial extension (`INSTALL spatial` / `LOAD spatial`) is connection-scoped — it must be loaded on every new DuckDB connection before GIS operations.
- All SQL identifiers from user input must be validated through `identifiers::validate_table_name` before use in SQL strings.
- `spatia_cli` is a dev/ops tool and is not shipped to end users; only `spatia` (the Tauri app) is the user-facing binary.
- The default CSV ingestion target is the `raw_staging` table (replaced on each load); named-table ingestion (`ingest_csv_to_table`) does not replace an existing table.
