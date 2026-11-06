# scripts

`release.ps1` cuts a release: preflight, gate, version bump, commits and tag,
release build, and the portable zip (same layout as `.github/workflows/release.yml`).

    powershell -ExecutionPolicy Bypass -File scripts/release.ps1 -Version 0.8.0 -Title "..."

Switches: `-SkipGate`, `-SkipBuild`, `-DryRun` (prints every command, changes nothing).
Update RELEASE_NOTES.md / TESTING.md first; the script commits them.
It never pushes; it prints the push command for you to run.
