# spatia

Desktop geo data app built with Tauri + React. The Rust backend is split into a core engine crate and a CLI wrapper so the same ingestion and processing logic can be reused.

Current implementation includes CSV ingestion/schema and transitional geocoding. The active roadmap is pivoting map + search toward Overture + DuckDB.

## Architecture

- `src-tauri/` - Tauri app crate
- `src-tauri/crates/engine/` - Core engine (`spatia_engine`)
- `src-tauri/crates/cli/` - CLI wrapper (`spatia_cli`)

The engine currently exposes a minimal CSV ingestion function that loads a CSV into a DuckDB table.

Target direction:

- Query Overture GeoParquet with DuckDB (bounded extraction)
- Build local PMTiles artifacts for map rendering
- Local geocoding via Overture search commands

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
cargo run -p spatia_cli -- schema ./spatia.duckdb places
```

If you want to pass a text line instead of args:

```bash
echo "ingest ./spatia.duckdb ./data/sample.csv places" | cargo run -p spatia_cli
```

### Overture extract + search (current)

```bash
cargo run -p spatia_cli -- overture_extract ./spatia.duckdb places place -122.4,47.5,-122.2,47.7 places_wa
cargo run -p spatia_cli -- overture_search ./spatia.duckdb places_wa "lincoln" 20
```

### PMTiles precompute workflow (external)

```bash
bash src-tauri/scripts/build_overture_pmtiles.sh ./src-tauri/spatia.duckdb places_wa places ./out/places.pmtiles 6 14 1
```

Requires `duckdb` CLI and `tippecanoe` in `PATH`.

## Testing the ingest

Sample CSV lives at `data/sample.csv`. It includes a few rows with `id`, `name`, `lat`, `lon`, and `category` columns for quick testing.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
