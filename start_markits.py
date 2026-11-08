"""Build and launch the MarkIts desktop app from any working directory."""

import argparse
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent
DESKTOP = ROOT / "apps" / "desktop"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="Use an optimized Rust build")
    parser.add_argument("--dry-run", action="store_true", help="Show commands without running them")
    args = parser.parse_args()

    npm = shutil.which("npm.cmd" if sys.platform == "win32" else "npm")
    cargo = shutil.which("cargo")
    if not npm or not cargo:
        print("Node.js (npm) and Rust (cargo) must be installed and available on PATH.", file=sys.stderr)
        return 1

    commands = []
    if not (DESKTOP / "node_modules").is_dir():
        commands.append(([npm, "ci"], DESKTOP))
    commands.append(([npm, "run", "build"], DESKTOP))
    launch = [cargo, "run", "--package", "markits-desktop"]
    if args.release:
        launch.append("--release")
    commands.append((launch, ROOT))

    try:
        for command, directory in commands:
            print(f"[{directory}] {subprocess.list2cmdline(command)}", flush=True)
            if not args.dry_run:
                subprocess.run(command, cwd=directory, check=True)
    except subprocess.CalledProcessError as error:
        return error.returncode
    except OSError as error:
        print(f"Failed to launch MarkIts: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    return 0


if __name__ == "__main__":
    sys.exit(main())
