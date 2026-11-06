#!/usr/bin/env python3
"""Keep cargo-deny and cargo-audit advisory exceptions documented and aligned."""
import re
import sys
import tomllib
from pathlib import Path

ID = re.compile(r"RUSTSEC-\d{4}-\d{4}", re.ASCII)
REASON = re.compile(r'"(RUSTSEC-\d{4}-\d{4})",\s*#\s*(\S.*)', re.ASCII)


def load_ignores(path: Path) -> dict[str, str]:
    """Load the TOML ignore IDs, retaining empty reasons for validation."""
    source = path.read_text()
    entries = tomllib.loads(source).get("advisories", {}).get("ignore", [])
    if not isinstance(entries, list) or any(not isinstance(item, str) for item in entries):
        raise ValueError(f"{path}: ignore must be a list of advisory IDs")
    # Only comments inside the advisories.ignore array document exceptions.
    reasons = {}
    in_advisories = False
    in_ignore = False
    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and not in_ignore:
            in_advisories = stripped == "[advisories]"
        if in_advisories and re.match(r"ignore\s*=\s*\[", stripped):
            in_ignore = True
        if in_ignore:
            match = REASON.search(line)
            if match:
                reasons[match[1]] = match[2].strip()
            if "]" in line.split("#", 1)[0]:
                in_ignore = False
    return {entry: reasons.get(entry, "") for entry in entries}


def main(deny: Path, audit: Path) -> int:
    """Print offending IDs and return 1 for invalid or differing ignores."""
    try:
        lists = [load_ignores(path) for path in (deny, audit)]
    except (OSError, ValueError) as error:
        print(error)
        return 1
    errors = []
    for path, entries in zip((deny, audit), lists):
        for advisory, reason in entries.items():
            if not ID.fullmatch(advisory):
                errors.append(f"{path}: malformed advisory ID {advisory}")
            elif not reason:
                errors.append(f"{path}: {advisory}: missing reason")
    for advisory in sorted(lists[0].keys() ^ lists[1].keys()):
        errors.append(f"ignore list mismatch: {advisory}")
    for error in errors:
        print(error)
    return int(bool(errors))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: check_advisory_ignores.py deny.toml .cargo/audit.toml")
    sys.exit(main(Path(sys.argv[1]), Path(sys.argv[2])))
