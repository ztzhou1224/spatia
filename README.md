# spatia

Desktop geo data app built with Tauri + React. The Rust backend is split into a core engine crate and a CLI wrapper so the same ingestion and processing logic can be reused.

## Architecture

- `src-tauri/` - Tauri app crate
- `src-tauri/crates/engine/` - Core engine (`spatia_engine`)
- `src-tauri/crates/cli/` - CLI wrapper (`spatia_cli`)

The engine currently exposes a minimal CSV ingestion function that loads a CSV into a DuckDB table.

## Development

### Desktop app

```bash
pnpm tauri dev
```

### Engine (core crate)

```bash
cargo check -p spatia_engine
```

### CLI wrapper

```bash
cargo run -p spatia_cli -- ingest ./spatia.duckdb ./data/sample.csv places
```

If you want to pass a text line instead of args:

```bash
echo "ingest ./spatia.duckdb ./data/sample.csv places" | cargo run -p spatia_cli
```

## Testing the ingest

Sample CSV lives at `data/sample.csv`. It includes a few rows with `id`, `name`, `lat`, `lon`, and `category` columns for quick testing.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
