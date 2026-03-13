Explore a Rust crate in the Spatia workspace and summarize its API.

Takes `$ARGUMENTS` as the crate name (e.g., `engine`, `ai`, `cli`, `bench`).

## Steps

1. Read `src-tauri/crates/$ARGUMENTS/Cargo.toml` to understand dependencies and feature flags.
2. Read `src-tauri/crates/$ARGUMENTS/src/lib.rs` (or `main.rs` for binaries) to find the public API surface.
3. Glob `src-tauri/crates/$ARGUMENTS/src/**/*.rs` to find all source files.
4. For each source file, identify:
   - Public types (`pub struct`, `pub enum`, `pub type`)
   - Public functions (`pub fn`, `pub async fn`)
   - Test modules (`#[cfg(test)]`)
   - Feature-gated code (`#[cfg(feature = "...")]`)

## Output

Provide a structured summary:

### Crate: spatia_$ARGUMENTS
- **Type**: library / binary
- **Feature gates**: list any
- **Dependencies**: key external crates
- **Public API**:
  - Types: list with one-line descriptions
  - Functions: list with signatures
- **Test modules**: list files with test coverage
- **Notes**: any interesting patterns or constraints
