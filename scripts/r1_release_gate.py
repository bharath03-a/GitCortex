#!/usr/bin/env python3
"""Fail-closed validation of checksummed, normalized R1 evidence summaries."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

MANDATORY_LANES = {"retrieval", "codex-graph-cli", "claude-code-mcp", "graphify", "codegraph"}
COMPETITOR_URLS = {"graphify": "https://github.com/Graphify-Labs/graphify", "codegraph": "https://github.com/colbymchenry/codegraph"}
CRATES = {"gitcortex", "gitcortex-core", "gitcortex-indexer", "gitcortex-store", "gitcortex-mcp", "gitcortex-viz"}
NPM_PACKAGES = {"gitcortex", "gitcortex-darwin-arm64", "gitcortex-darwin-x64", "gitcortex-linux-arm64", "gitcortex-linux-x64"}


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def pinned(value, length):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{%d}" % length, value) is not None


def source_tree_hash(root, revision="HEAD"):
    tree = subprocess.run(["git", "ls-tree", "-r", "-z", revision], cwd=root, capture_output=True, check=True).stdout
    digest = hashlib.sha256()
    for entry in tree.split(b"\0"):
        if not entry:
            continue
        path = entry.split(b"\t", 1)[1].decode("utf-8", errors="surrogateescape")
        if path.startswith(("docs/", "tools/agent-bench/results/")) or path in {"README.md", "CHANGELOG.md", "tools/agent-bench/r1-release-manifest.json"}:
            continue
        digest.update(entry + b"\0")
    return digest.hexdigest()


def validate(root, manifest, versions, dirty):
    errors = []
    if dirty:
        errors.append("release candidate worktree is dirty (including untracked files)")
    if not versions or len(set(versions.values())) != 1:
        errors.append(f"package versions disagree: {versions}")
    for key, length in [("candidate_commit", 40), ("binary_sha256", 64), ("suite_sha256", 64), ("source_tree_sha256", 64)]:
        if not pinned(manifest.get(key), length):
            errors.append(f"manifest requires pinned {key}")
    lanes = manifest.get("required_lanes")
    if not isinstance(lanes, list) or any(not isinstance(lane, str) for lane in lanes) or not MANDATORY_LANES.issubset(lanes):
        errors.append("required_lanes must include retrieval, Codex CLI, Claude MCP, Graphify, and CodeGraph")
        lanes = []
    suite = {}
    competitors = {}
    for path_key, hash_key in [("suite_path", "suite_sha256"), ("binary_path", "binary_sha256")]:
        try:
            path = (root / manifest[path_key]).resolve()
            path.relative_to(root.resolve())
            if sha256(path) != manifest.get(hash_key):
                raise ValueError(f"{path_key} checksum differs from manifest")
            if path_key == "suite_path":
                suite = tomllib.loads(path.read_text())
        except (OSError, ValueError, KeyError, TypeError) as error:
            errors.append(str(error))
    try:
        expected_tasks = {task["id"] for task in suite["tasks"]}
        expected_repos = {repo["name"]: repo["commit"] for repo in suite["repos"]}
        if not expected_tasks or not expected_repos:
            raise ValueError("suite must contain pinned repositories and tasks")
    except (KeyError, TypeError, ValueError) as error:
        errors.append(f"invalid pinned suite: {error}")
        expected_tasks, expected_repos = set(), {}
    try:
        path = (root / manifest["competitors_path"]).resolve()
        path.relative_to(root.resolve())
        if sha256(path) != manifest.get("competitors_sha256"):
            raise ValueError("competitor manifest checksum differs")
        competitors = tomllib.loads(path.read_text()) if path.suffix == ".toml" else json.loads(path.read_text())
        for name, url in COMPETITOR_URLS.items():
            pin = competitors[name]
            runtimes = pin.get("runtime_versions")
            install = pin.get("install_command")
            if pin.get("url") != url or not pinned(pin.get("commit"), 40) or not isinstance(install, str) or not install.strip() or not isinstance(runtimes, dict) or not runtimes or any(not isinstance(value, str) or not value.strip() for value in runtimes.values()):
                raise ValueError(f"{name}: canonical URL, exact commit, install command, and runtime versions required")
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        errors.append(f"invalid competitor manifest: {error}")
        competitors = {}
    results = manifest.get("results", [])
    if not isinstance(results, list):
        errors.append("results must be a list")
        results = []
    seen = {}
    found_lanes = set()
    for entry in results:
        try:
            path = (root / entry["path"]).resolve()
            path.relative_to(root.resolve())
            if not pinned(entry.get("sha256"), 64) or sha256(path) != entry["sha256"]:
                raise ValueError("summary checksum mismatch")
            result = json.loads(path.read_text())
            if not isinstance(result, dict):
                raise ValueError("summary must be an object")
        except (OSError, ValueError, KeyError, TypeError) as error:
            errors.append(f"result {entry}: {error}")
            continue
        identifier = result.get("id")
        lane = result.get("lane")
        if not isinstance(identifier, str) or not identifier or identifier in seen:
            errors.append(f"result requires unique id: {identifier}")
            continue
        seen[identifier] = result
        artifacts = result.get("artifacts")
        if not isinstance(artifacts, list) or not artifacts:
            errors.append(f"{identifier}: checksummed raw artifacts required")
            artifacts = []
        for artifact in artifacts:
            try:
                artifact_path = (root / artifact["path"]).resolve()
                artifact_path.relative_to(root.resolve())
                if not pinned(artifact.get("sha256"), 64) or sha256(artifact_path) != artifact["sha256"]:
                    raise ValueError("raw artifact checksum mismatch")
            except (OSError, ValueError, KeyError, TypeError) as error:
                errors.append(f"{identifier}: {error}")
        if isinstance(lane, str):
            found_lanes.add(lane)
        else:
            errors.append(f"{identifier}: missing lane")
        competitor = lane in COMPETITOR_URLS if isinstance(lane, str) else False
        if competitor and (not result.get("competitor") or result["competitor"] != competitors.get(lane)):
            errors.append(f"{identifier}: competitor identity/runtime differs from pinned manifest")
        for key in ["candidate_commit", "binary_sha256", "suite_sha256"]:
            if not result.get(key) or result[key] != manifest.get(key):
                errors.append(f"{identifier}: {key} missing or differs from candidate")
        repos = result.get("repo_commits")
        if not isinstance(repos, dict) or not repos or any(not pinned(value, 40) for value in repos.values()):
            errors.append(f"{identifier}: exact repo_commits required")
        if repos != expected_repos:
            errors.append(f"{identifier}: repo_commits differ from pinned suite")
        for key in ["client", "model", "cache_state"]:
            if not isinstance(result.get(key), str) or not result[key].strip():
                errors.append(f"{identifier}: missing {key}")
        native_client = {"codex-graph-cli": "codex", "claude-code-mcp": "claude"}.get(lane) if isinstance(lane, str) else None
        model = result.get("model")
        if native_client and (result.get("client") != native_client or not isinstance(model, str) or model in {"", "none", "unknown", "pending"}):
            errors.append(f"{identifier}: native lane needs its actual client and model")
        rounds = result.get("rounds")
        minimum = 1 if lane == "retrieval" else 3
        if type(rounds) is not int or not minimum <= rounds <= 100:
            errors.append(f"{identifier}: {minimum}..100 rounds required")
        tasks = result.get("tasks")
        if not isinstance(tasks, list) or not tasks:
            errors.append(f"{identifier}: nonempty tasks required")
            tasks = []
        ids = set()
        supported_tasks = 0
        for task in tasks:
            if not isinstance(task, dict) or not isinstance(task.get("id"), str) or not task["id"]:
                errors.append(f"{identifier}: malformed task")
                continue
            if task["id"] in ids:
                errors.append(f"{identifier}: duplicate task {task['id']}")
            ids.add(task["id"])
            if competitor and task.get("outcome") == "unsupported":
                if not isinstance(task.get("reason"), str) or not task["reason"].strip():
                    errors.append(f"{identifier}/{task['id']}: unsupported workflow needs a reason")
                continue
            supported_tasks += 1
            if task.get("valid") is not True or (not competitor and task.get("quality_non_inferior") is not True):
                errors.append(f"{identifier}/{task['id']}: invalid or quality regression")
            samples = task.get("samples")
            if not isinstance(samples, list) or not samples:
                errors.append(f"{identifier}/{task['id']}: per-round samples required")
                samples = []
            sample_rounds = set()
            for sample in samples:
                if not isinstance(sample, dict) or type(sample.get("round")) is not int:
                    errors.append(f"{identifier}/{task['id']}: malformed round")
                    continue
                if sample["round"] in sample_rounds or sample.get("valid") is not True or (not competitor and sample.get("quality_non_inferior") is not True):
                    errors.append(f"{identifier}/{task['id']}: duplicate, invalid, or regressed round")
                sample_rounds.add(sample["round"])
            if type(rounds) is not int or not 1 <= rounds <= 100 or sample_rounds != set(range(1, rounds + 1)):
                errors.append(f"{identifier}/{task['id']}: incomplete round coverage")
        if ids != expected_tasks:
            errors.append(f"{identifier}: task coverage differs from pinned suite")
        if competitor and not supported_tasks:
            errors.append(f"{identifier}: at least one valid supported workflow is required")
        if lane == "retrieval" and result.get("relevance_non_inferior") is not True:
            errors.append(f"{identifier}: relevance_non_inferior must pass")
    for lane in lanes:
        if lane not in found_lanes:
            errors.append(f"missing required lane: {lane}")
    claims = manifest.get("claims", [])
    if not isinstance(claims, list):
        errors.append("claims must be a list")
        claims = []
    for claim in claims:
        if not isinstance(claim, dict):
            errors.append("malformed claim")
            continue
        result_id = claim.get("result_id")
        result = seen.get(result_id) if isinstance(result_id, str) else None
        task_ids = claim.get("task_ids")
        result_tasks = result.get("tasks") if result else []
        known = {t["id"] for t in result_tasks if isinstance(t, dict) and isinstance(t.get("id"), str) and t.get("valid") is True and t.get("outcome") != "unsupported"} if isinstance(result_tasks, list) else set()
        if not claim.get("id") or not isinstance(task_ids, list) or not task_ids or any(not isinstance(t, str) or t not in known for t in task_ids):
            errors.append(f"claim {claim.get('id')}: matching result and task_ids required")
    return errors


def package_versions(root):
    cargo = tomllib.loads((root / "Cargo.toml").read_text())
    versions = {"Cargo.toml": cargo["workspace"]["package"]["version"]}
    minor = ".".join(versions["Cargo.toml"].split(".")[:2])
    for name in CRATES - {"gitcortex"}:
        requirement = cargo["workspace"]["dependencies"][name]["version"]
        if requirement not in {minor, versions["Cargo.toml"]}:
            raise ValueError(f"workspace dependency {name} requirement {requirement} does not match {minor}")
    members = {tomllib.loads((root / member / "Cargo.toml").read_text())["package"]["name"]:
               tomllib.loads((root / member / "Cargo.toml").read_text())["package"] for member in cargo["workspace"]["members"]}
    if set(members) != CRATES or any(package.get("version") != {"workspace": True} for package in members.values()):
        raise ValueError("workspace crate inventory or inherited versions differ")
    for name in sorted(NPM_PACKAGES):
        path = root / "npm/packages" / name / "package.json"
        data = json.loads(path.read_text())
        expected_name = "gitcortex" if name == "gitcortex" else "@gitcortex/gcx-" + name.removeprefix("gitcortex-")
        if data.get("name") != expected_name:
            raise ValueError(f"unexpected npm package name in {path}")
        if name == "gitcortex":
            expected_optional = {"@gitcortex/gcx-" + item.removeprefix("gitcortex-") for item in NPM_PACKAGES - {"gitcortex"}}
            if set(data.get("optionalDependencies", {})) != expected_optional:
                raise ValueError("npm umbrella optional package inventory differs")
        versions[str(path.relative_to(root))] = data["version"]
        for name, version in data.get("optionalDependencies", {}).items():
            if name.startswith("@gitcortex/"):
                versions[f"{path.relative_to(root)}:{name}"] = version
    path = root / "python/src/gitcortex/__init__.py"
    match = re.search(r'^__version__\s*=\s*[\'"]([^\'"]+)', path.read_text(), re.MULTILINE)
    if not match:
        raise ValueError("Python __version__ missing")
    versions[str(path.relative_to(root))] = match[1]
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    found = set()
    for package in lock.get("package", []):
        if package["name"] in CRATES:
            if package["name"] in found:
                raise ValueError("duplicate workspace crate in Cargo.lock")
            found.add(package["name"])
            versions[f"Cargo.lock:{package['name']}"] = package["version"]
    if found != CRATES:
        raise ValueError("Cargo.lock is missing workspace crates")
    return versions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path("tools/agent-bench/r1-release-manifest.json"))
    parser.add_argument("--source-tree", action="store_true", help="print the current candidate input-tree fingerprint")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    if args.source_tree:
        print(source_tree_hash(root))
        return 0
    try:
        manifest = json.loads((root / args.manifest).read_text())
        if not isinstance(manifest, dict):
            raise ValueError("manifest must be an object")
        status = subprocess.run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=root, capture_output=True, text=True, check=True)
        head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True, check=True).stdout.strip()
        errors = validate(root, manifest, package_versions(root), bool(status.stdout))
        # Compare input trees, not ancestry: squash merging removes the approved
        # candidate's ancestry even when its code is unchanged.
        candidate = manifest.get("candidate_commit", "")
        if pinned(candidate, 40):
            expected_tree = manifest.get("source_tree_sha256")
            if not pinned(expected_tree, 64) or source_tree_hash(root, candidate) != expected_tree or source_tree_hash(root, head) != expected_tree:
                errors.append("candidate input tree differs from approved evidence; rebuild and rerun")
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        errors = [str(error)]
    print(json.dumps({"ready": not errors, "errors": errors}, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
