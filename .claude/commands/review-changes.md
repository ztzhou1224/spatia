Review uncommitted changes against Spatia project conventions.

## Steps

1. Run `git diff` and `git diff --cached` to see all uncommitted changes (staged and unstaged).
2. Review the diff against these project conventions:
   - **SQL safety**: All user-input SQL identifiers must be validated via `identifiers.rs` — no raw interpolation.
   - **Error handling**: Rust engine functions must return `EngineResult<T>`. No `unwrap()` in library code.
   - **TypeScript strictness**: No `any` types unless absolutely necessary with a comment explaining why.
   - **Temp file cleanup**: Test code creating DuckDB files must clean up `.duckdb`, `.wal`, and `.wal.lck`.
   - **Analysis SQL prefix**: Any SQL execution must enforce `CREATE [OR REPLACE] VIEW analysis_result AS ...`.
   - **DuckDB extensions**: `spatial` and `httpfs` must be loaded per connection (connection-scoped).
   - **No architecture rewrites**: Changes must not rewrite core architecture or DB schemas without explicit permission.

3. Report findings grouped by severity:
   - **CRITICAL**: SQL injection risk, data loss potential, missing safety checks
   - **WARNING**: Convention violations, missing error handling, `any` types
   - **INFO**: Style suggestions, minor improvements

If no issues are found, report "All changes look good."
