Run tests for a specific module or crate.

Takes `$ARGUMENTS` as either a crate name or a test filter pattern.

## Logic

1. If `$ARGUMENTS` matches a known crate name (`engine`, `ai`, `cli`, `bench`):
   - Run `cd src-tauri && cargo test -p spatia_$ARGUMENTS` and report results.

2. Otherwise, treat `$ARGUMENTS` as a test name filter:
   - Run `cd src-tauri && cargo test --workspace -- $ARGUMENTS` and report results.

## Output

Report:
- Number of tests run, passed, failed, ignored
- For any failures: show the test name and failure output
- If all pass: confirm with a one-line summary
