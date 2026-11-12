# Markits Windows v0.8.5

- Add CLI operation recording and replay, including pause/resume, screenshot points, and validation without sending input.
- Add `capture-run` to launch executables or embedded scripts with literal arguments and a working directory, operate a selected window, and capture screenshots automatically.
- Embed versioned recapture recipes containing script contents, arguments, working directory, window bounds, and operation recordings. Use `recapture` to reproduce captures, or `recipe` to extract reusable JSON without executing it.
- Preserve editable annotations and cropping across recapture, and support lossless WebP/JPEG XL export and desktop capture history from the CLI.
- Share Java Access Bridge UI detection and Windows cursor rendering between the CLI and desktop. Add configurable UI exploration depth and stable native window IDs.
- Handle already-active windows and improve foreground activation for automated window operations on Windows.

## Downloads

- `markits-cli_0.8.5_windows-x64.zip`: standalone Windows CLI, README, bundled-manual reference, and license. Run `markits.exe --help` or `markits.exe manual`.
- `markits-desktop_0.8.5_x64-setup.exe`: Windows desktop installer.
- `SHA256SUMS.txt`: SHA-256 checksums for the release downloads.

Recapture explicitly executes the saved startup script and recorded input. `recipe` and `--dry-run` inspect recipes without executing them. Windows capture and operation recording require desktop access.
