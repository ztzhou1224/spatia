Analyze changes, create a conventional commit, and push.

## Steps

1. Run `git status` and `git diff` to understand all changes.
2. Run `git log --oneline -10` to see recent commit message style.
3. Analyze the changes and generate a conventional commit message:
   - Use the format: `type(scope): message`
   - Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`
   - Scope: the most relevant module (`engine`, `ai`, `map`, `chat`, `geocode`, `bench`, `ui`, etc.)
   - Message: concise description of what changed and why
   - Follow the style of recent commits (e.g., `fix(bench): resolve CSV paths`, `feat(geocode): add fuzzy search`)
4. Stage the relevant files (prefer specific files over `git add -A`; never stage `.env` or credential files).
5. Create the commit with the message, ending with:
   `Co-Authored-By: Claude <noreply@anthropic.com>`
6. Push to the remote: `git push`

## Output

Report the commit hash, message, and push status.
