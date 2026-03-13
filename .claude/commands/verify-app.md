Verify the running Spatia app by taking a screenshot and describing its visible state.

## Steps

1. Run `bash scripts/ensure-app-running.sh` to make sure the app is running. If it fails, report the error and stop.
2. Run `bash scripts/capture-app.sh` to take a screenshot.
3. Read the screenshot PNG file and describe the visible UI state:
   - Is the map rendering correctly?
   - Is the right panel (FileList) visible and showing tables?
   - Is the chat card visible at the bottom?
   - Are there any error states, blank areas, or visual glitches?
   - What data appears to be loaded (if any)?

4. If `bash scripts/dump-ui-state.sh` exists and works, also run it and summarize the Zustand store state.

## Output

Provide a brief visual status report with any issues noted.
