__version__ = "0.7.2"

import os
import stat
import subprocess
import sys


def _binary_path() -> str:
    bin_name = "gcx.exe" if sys.platform == "win32" else "gcx"
    return os.path.join(os.path.dirname(__file__), "_bin", bin_name)


def main() -> None:
    binary = _binary_path()
    if not os.path.isfile(binary):
        sys.exit(
            f"gcx binary not found at {binary}.\n"
            "Try reinstalling: pip install --force-reinstall gitcortex"
        )
    # Ensure the binary is executable (wheels may not preserve the bit)
    current = os.stat(binary).st_mode
    os.chmod(binary, current | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    try:
        sys.exit(subprocess.call([binary] + sys.argv[1:]))
    except KeyboardInterrupt:
        # Ctrl-C (e.g. stopping `gcx viz`'s server) delivers SIGINT to this
        # process too; the child already exits cleanly on its own, so just
        # exit quietly at the conventional 128+SIGINT code instead of
        # printing a Python traceback for a normal interactive stop.
        sys.exit(130)
