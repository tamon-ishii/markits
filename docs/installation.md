# Desktop installation

Download the desktop package for your OS from the GitHub Release assets.

| OS | Package | Installation |
| --- | --- | --- |
| Windows | `*-setup.exe` | Run the setup wizard. Installs for the current user. |
| macOS | `*.dmg` | Open the disk image and drag MarkIts into Applications. Supports Apple Silicon and Intel. |
| Debian / Ubuntu | `*.deb` | Install with `sudo apt install ./<package>.deb`. |
| Fedora / RPM distributions | `*.rpm` | Install with `sudo dnf install ./<package>.rpm`. |
| Other Linux distributions | `*.AppImage` | Make executable and run directly; no package installation required. |

## Windows login startup

The Windows setup installer creates `MarkIts.lnk` in the current user's Startup
folder. At the next login, MarkIts starts in the system tray without opening the
editor. The capture shortcut and tray menu remain available. Opening MarkIts
normally displays the editor.

To disable login startup, open `shell:startup` from Windows Run and remove
`MarkIts.lnk`. Reinstalling or upgrading restores the shortcut. Uninstalling
MarkIts removes it automatically.

macOS and Linux packages do not automatically register login startup.

## Building packages

Run `npm ci` and `npm run tauri build` from `apps/desktop` on the target OS.
Windows uses the NSIS setup installer configured in `tauri.windows.conf.json`;
macOS and Linux use Tauri's platform bundle targets. Release tag pushes build
these packages on native GitHub Actions runners and attach them to the release.
