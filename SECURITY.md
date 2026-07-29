# Security policy

## Supported versions

GitCortex is pre-1.0. Only the latest released version receives security fixes.
The current version is listed in the
[release log](https://bharath03-a.github.io/GitCortex/releases.html).

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's
[report a vulnerability](https://github.com/bharath03-a/GitCortex/security/advisories/new)
form. Please do not open a public issue for a security problem.

Include the version, the platform, what an attacker can do, and the smallest
reproduction you have. A repository that triggers the problem is more useful
than a description of it.

Expect an acknowledgement within a week. If a report is accepted, the fix and
the advisory are published together.

## Threat model

GitCortex indexes source code on a developer's machine and serves the resulting
graph to local AI assistants. Two properties matter most.

**Source code must not leave the machine.** Indexing, storage, embeddings, and
query answering are all local. There is no telemetry and no network call in the
indexing or query path. A defect that causes source code, file paths, or graph
contents to be transmitted anywhere is a vulnerability, not a bug.

**Indexing an untrusted repository must be safe.** `gcx` parses whatever is in
the working tree, so a hostile repository is untrusted input. Path traversal
outside the repository or the graph directory, code execution during parsing or
indexing, and crashes that corrupt an existing graph all qualify.

The MCP server binds to stdio and is reachable only by the process that spawned
it. Anything that widens that surface — a network listener, or a tool that
returns file contents from outside the indexed repository — qualifies.

## Out of scope

- Vulnerabilities in dependencies along paths GitCortex never reaches. Report
  those upstream, and tell us if GitCortex exposes them.
- Denial of service from indexing a deliberately pathological repository, unless
  it corrupts the graph or escapes the repository directory.
- Automated scanner output with no demonstrated impact.
