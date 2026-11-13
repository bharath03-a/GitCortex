#!/usr/bin/env python3
"""Fail-closed gate: every release claim must cite a verified eval result.

Sibling to the R1 evidence gate (that script is not on `main`). A manifest lists
eval summaries (made by tools/agent-bench/eval_report.py), the candidate and
baseline binary hashes, and the claims the release will make. The gate checks
file and raw-trace checksums, provenance, completeness, rounds, correctness
gates, and recomputes each claim from the cited summary. Docs listed in the
manifest may not state a performance ratio without a `[result: id]` citation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import re
import sys
from pathlib import Path
from typing import Any

DEFAULT_MANIFEST = "tools/agent-bench/eval-release-manifest.json"
HEX = {64: re.compile(r"^[0-9a-f]{64}$"), 40: re.compile(r"^[0-9a-f]{40}$")}
OPS = {">=": operator.ge, "<=": operator.le, ">": operator.gt, "<": operator.lt, "==": operator.eq}
CITATION = re.compile(r"\[result:\s*([A-Za-z0-9._-]+)\s*\]")
PERF_CLAIM = re.compile(
    r"\d+(?:\.\d+)?\s?(?:x|×)\b|\d+\s?%\s*(?:fewer|less|lower|cheaper|faster|reduction|savings)", re.I
)
REQUIRED_LANES = {
    "candidate": ("claude-code-unforced", "claude-code-continuous"),
    "baseline": ("claude-code-unforced",),
}
PROVENANCE_TEXT = ("client", "client_version", "model", "created_at", "gcx_version")


def pinned(value: Any, length: int) -> bool:
    return isinstance(value, str) and HEX[length].match(value) is not None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def logs_digest(logs: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(p for p in logs.rglob("*") if p.is_file()):
        digest.update(f"{path.relative_to(logs)}\0{sha256_file(path)}\n".encode())
    return digest.hexdigest()


def inside(root: Path, relative: str) -> Path:
    path = (root / relative).resolve()
    path.relative_to(root.resolve())
    return path


def check_trace(root: Path, label: str, trace: dict[str, Any], binary: str | None, min_rounds: int) -> list[str]:
    errors = []
    for key in ("suite_sha256", "core_suite_sha256"):
        if not pinned(trace.get(key), 64):
            errors.append(f"{label}: {key} must be a pinned sha256")
    commits = trace.get("repo_commits")
    if not isinstance(commits, dict) or not commits or any(not pinned(v, 40) for v in commits.values()):
        errors.append(f"{label}: exact repo_commits required")
    for key in PROVENANCE_TEXT:
        if not isinstance(trace.get(key), str) or not trace[key].strip():
            errors.append(f"{label}: missing {key}")
    if binary is None or trace.get("gcx_sha256") != binary:
        errors.append(f"{label}: gcx_sha256 does not match the manifest binary for this role")
    harness = trace.get("harness") or {}
    if harness.get("dirty") is not False or not pinned(harness.get("commit"), 40):
        errors.append(f"{label}: harness must be a clean, pinned commit")
    if trace.get("aborted") is not None or trace.get("planned_runs") != trace.get("completed_runs"):
        errors.append(
            f"{label}: incomplete run (planned {trace.get('planned_runs')}, completed {trace.get('completed_runs')})"
        )
    if not isinstance(trace.get("rounds"), int) or trace["rounds"] < min_rounds:
        errors.append(f"{label}: {min_rounds}+ rounds required, found {trace.get('rounds')}")
    return errors


def check_artifacts(root: Path, label: str, summary: dict[str, Any]) -> list[str]:
    errors = []
    for artifact in summary.get("artifacts", []):
        try:
            path = inside(root, artifact["path"])
            if sha256_file(path) != artifact["sha256"]:
                errors.append(f"{label}: raw trace checksum mismatch: {artifact['path']}")
        except (OSError, ValueError, KeyError, TypeError):
            errors.append(f"{label}: raw trace missing or outside repo: {artifact}")
    for trace in summary.get("traces", []):
        logs = trace.get("logs")
        if not logs:
            errors.append(f"{label}: {trace.get('label')} has no raw client logs recorded")
            continue
        try:
            if logs_digest(inside(root, logs["path"])) != logs["sha256"]:
                errors.append(f"{label}: raw log checksum mismatch: {logs['path']}")
        except (OSError, ValueError, KeyError, TypeError):
            errors.append(f"{label}: raw logs missing or outside repo: {logs}")
    return errors


def check_gates(label: str, summary: dict[str, Any]) -> tuple[list[str], list[str]]:
    """Section 4A correctness gates on a candidate summary. Returns (errors, losses)."""
    errors: list[str] = []
    losses: list[str] = []
    for dim in ("overall", "workflow", "lane", "language", "repo"):
        for key, block in summary["groups"].get(dim, {}).items():
            where = f"{label}[{dim}={key}]"
            for arm, metrics in block["arms"].items():
                if metrics["false_safe"]:
                    errors.append(f"{where}/{arm}: false-safe claims must be 0, found {metrics['false_safe']}")
            for arm, paired in block["paired"].items():
                if paired["non_inferior"] is False:
                    errors.append(f"{where}/{arm}: success is not non-inferior to baseline")
                ratio = paired["ratios"]["uncached_tokens"]["geomean"]
                if ratio is not None and ratio < 1.0:
                    (errors if dim == "overall" else losses).append(f"{where}/{arm}: uncached ratio {ratio} < 1.0")
    return errors, losses


def metric_value(block: dict[str, Any], arm: str, metric: str) -> float | None:
    if metric.endswith("_ratio"):
        return block["paired"][arm]["ratios"][metric.removesuffix("_ratio")]["geomean"]
    if metric == "success_delta":
        return block["paired"][arm]["success_delta"]
    arm_block = block["arms"][arm]
    if metric == "false_safe":
        return arm_block["false_safe"]
    if metric == "success":
        return arm_block["success"]["rate"]
    return arm_block["discoverability"][metric]["rate"]


def check_claim(claim: dict[str, Any], results: dict[str, dict[str, Any]]) -> list[str]:
    name = claim.get("id") or "<unnamed>"
    summary = results.get(claim.get("result_id", ""))
    if summary is None:
        return [f"claim {name}: cites unknown result id {claim.get('result_id')!r}"]
    if not summary.get("variance_ok"):
        return [f"claim {name}: result {summary['id']} has fewer than {summary.get('min_rounds')} rounds"]
    if claim.get("op") not in OPS or not isinstance(claim.get("value"), (int, float)):
        return [f"claim {name}: needs op in {sorted(OPS)} and a numeric value"]
    try:
        dim, key = claim["group"].split("/", 1)
        actual = metric_value(summary["groups"][dim][key], claim["arm"], claim["metric"])
    except (KeyError, TypeError, ValueError, AttributeError):
        return [f"claim {name}: group/arm/metric not found in result {summary['id']}"]
    if actual is None or not OPS[claim["op"]](actual, claim["value"]):
        return [f"claim {name}: {claim['metric']} is {actual}, not {claim['op']} {claim['value']}"]
    return []


def check_docs(root: Path, docs: list[str], known: set[str]) -> list[str]:
    errors = []
    for relative in docs:
        try:
            lines = inside(root, relative).read_text(encoding="utf-8").splitlines()
        except (OSError, ValueError):
            errors.append(f"doc {relative}: missing or outside repo")
            continue
        for number, line in enumerate(lines, 1):
            cited = CITATION.findall(line)
            errors += [f"doc {relative}:{number}: cites unknown result id {c!r}" for c in cited if c not in known]
            if PERF_CLAIM.search(line) and not cited:
                errors.append(f"doc {relative}:{number}: performance claim without a [result: id] citation")
    return errors


def load_results(
    root: Path, manifest: dict[str, Any]
) -> tuple[dict[str, dict[str, Any]], dict[str, str], list[str]]:
    results: dict[str, dict[str, Any]] = {}
    roles: dict[str, str] = {}
    errors: list[str] = []
    for entry in manifest.get("results", []):
        try:
            path = inside(root, entry["path"])
            if not pinned(entry.get("sha256"), 64) or sha256_file(path) != entry["sha256"]:
                raise ValueError("summary checksum mismatch")
            summary = json.loads(path.read_text(encoding="utf-8"))
            if summary.get("type") != "eval-summary" or not summary.get("id"):
                raise ValueError("not an eval summary")
            if summary["id"] in results:
                raise ValueError(f"duplicate result id {summary['id']}")
            if entry.get("role") not in REQUIRED_LANES:
                raise ValueError("role must be candidate or baseline")
        except (OSError, ValueError, KeyError, TypeError) as error:
            errors.append(f"result {entry}: {error}")
            continue
        results[summary["id"]] = summary
        roles[summary["id"]] = entry["role"]
    return results, roles, errors


def validate(root: Path, manifest: dict[str, Any], docs: list[str] | None = None) -> tuple[list[str], list[str]]:
    results, roles, errors = load_results(root, manifest)
    losses: list[str] = []
    binaries = {
        "candidate": (manifest.get("candidate") or {}).get("gcx_sha256"),
        "baseline": (manifest.get("baseline") or {}).get("gcx_sha256"),
    }
    for role, value in binaries.items():
        if not pinned(value, 64):
            errors.append(f"manifest requires a pinned {role}.gcx_sha256")
    min_rounds = manifest.get("min_rounds", 3)
    if not isinstance(min_rounds, int) or min_rounds < 3:
        errors.append("min_rounds must be an integer of at least 3")
        min_rounds = 3
    seen_lanes: dict[str, set[str]] = {"candidate": set(), "baseline": set()}
    for result_id, summary in results.items():
        role = roles[result_id]
        for trace in summary.get("traces", []):
            seen_lanes[role].add(trace.get("lane"))
            errors += check_trace(root, result_id, trace, binaries[role], min_rounds)
        errors += check_artifacts(root, result_id, summary)
        if not (summary.get("complete") and summary.get("variance_ok") and summary.get("release_grade_harness")):
            errors.append(f"{result_id}: summary is incomplete, under-replicated, or from a dirty harness")
        if role == "candidate":
            gate_errors, gate_losses = check_gates(result_id, summary)
            errors += gate_errors
            losses += gate_losses
    for role, required in REQUIRED_LANES.items():
        errors += [f"missing {role} result for lane {lane}" for lane in required if lane not in seen_lanes[role]]
    claims = manifest.get("claims", [])
    if not isinstance(claims, list) or not claims:
        errors.append("manifest must list the claims the release makes (each cites a result id)")
        claims = []
    for claim in claims:
        errors += check_claim(claim, results) if isinstance(claim, dict) else ["malformed claim"]
    errors += check_docs(root, list(docs if docs is not None else manifest.get("docs", [])), set(results))
    return errors, losses


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", default=DEFAULT_MANIFEST)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    try:
        manifest = json.loads((args.root / args.manifest).read_text(encoding="utf-8"))
        if not isinstance(manifest, dict):
            raise ValueError("manifest must be an object")
        errors, losses = validate(args.root, manifest)
    except (OSError, ValueError, KeyError, TypeError) as error:
        errors, losses = [str(error)], []
    print(json.dumps({"ready": not errors, "errors": errors, "losses": losses}, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
