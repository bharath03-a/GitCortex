#!/usr/bin/env python3
"""Keep cargo-deny and cargo-audit advisory exceptions documented and aligned."""
import re
import sys
import tomllib
from pathlib import Path

ID = re.compile(r"RUSTSEC-\d{4}-\d{4}", re.ASCII)
# Tokenize strings before comments so punctuation inside strings is harmless.
TOKEN = re.compile(r"""\"(?:\\.|[^"\\])*\"|'[^']*'|\#[^\n]*|[\[\]{}]""", re.ASCII)


def load_ignores(path: Path) -> dict[str, str]:
    """Read values with TOML, and associate string entries with line comments."""
    source = path.read_text()
    entries = tomllib.loads(source).get("advisories", {}).get("ignore", [])
    if not isinstance(entries, list):
        raise ValueError(f"{path}: ignore must be a list of advisory IDs")
    reasons = {}
    in_advisories = False
    in_ignore = False
    depth = 0
    objects = 0
    comments = []
    for line in source.splitlines():
        stripped = line.strip()
        if not in_ignore and stripped.startswith("["):
            in_advisories = bool(re.fullmatch(r"\[advisories\]\s*(?:#.*)?", stripped))
        if in_advisories and re.match(r"ignore\s*=\s*\[", stripped):
            in_ignore = True
        if not in_ignore:
            continue
        ids = []
        comment = ""
        for match in TOKEN.finditer(line):
            token = match.group()
            if token.startswith("#"):
                comment = token[1:].strip()
                break
            if token == "[":
                depth += 1
            elif token == "]":
                depth -= 1
            elif token == "{":
                objects += 1
            elif token == "}":
                objects -= 1
            elif depth == 1 and objects == 0:
                ids.append(tomllib.loads("value = " + token)["value"])
        if len(ids) > 1:
            raise ValueError(f"{path}: multiple IDs on one line need separate reasons; "
                             "use one ID per line or objects with reason keys")
        comments.extend((advisory, comment) for advisory in ids)
        if depth == 0:
            in_ignore = False
    string_reasons = dict(comments)
    for entry in entries:
        if isinstance(entry, str):
            advisory, reason = entry, string_reasons.get(entry, "")
        elif isinstance(entry, dict) and isinstance(entry.get("id"), str):
            advisory, reason = entry["id"], entry.get("reason", "")
            if not isinstance(reason, str):
                raise ValueError(f"{path}: {advisory}: reason must be a string")
        else:
            raise ValueError(f"{path}: ignore entries must be IDs or objects with id and reason")
        if advisory in reasons:
            raise ValueError(f"{path}: duplicate advisory ID {advisory}")
        reasons[advisory] = reason.strip()
    return reasons


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
