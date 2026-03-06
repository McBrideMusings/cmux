---
name: read-logs
description: Read runtime logs from the last ./admin run. Use when the user says they ran the app and something didn't work, or when you need to check what happened during the last run.
---

# Read Logs

The log file is at `tmp/run.log` in the project root. It captures all stdout/stderr from the last `./admin` run.

## Strategy

Determine whether this is a **build problem** or a **runtime/logging problem**, then read accordingly.

### Build problem (app didn't launch, crash on start)
Read from the **top** of the log file (first 80 lines). Look for:
- `error:` lines from build tools
- `BUILD FAILED` or equivalent
- Crash output immediately after launch

### Runtime / behavior bug (app launched but something went wrong)
Read from the **bottom** of the log file (last 80 lines). The user typically quits after observing the bug.

### If you need more context
- Read the full file only if the targeted read didn't give enough info
- Search for specific error patterns

### What NOT to do
- Don't read the entire log file upfront if it's large
- Don't ask the user to paste logs -- just read the file
