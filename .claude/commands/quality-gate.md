Run the full Spatia quality gate and report pass/fail per step.

## Steps

1. Run `pnpm build` from the project root. Report PASS or FAIL.
2. Run `cargo test --workspace` from `src-tauri/`. Report PASS or FAIL with test count.
3. Run `cargo clippy --workspace` from `src-tauri/`. Report PASS or FAIL, listing any warnings.

## Output

Report a summary table:

| Step | Status | Details |
|------|--------|---------|
| pnpm build | PASS/FAIL | ... |
| cargo test | PASS/FAIL | X tests passed |
| cargo clippy | PASS/FAIL | X warnings |

If any step fails, show the relevant error output and stop — do not proceed to later steps.
