## What and why

<!-- What does this change, and which C function or file does it follow (lib/.../file.c:function)? -->

## Evidence

<!-- For a fidelity fix: the input that showed the difference, and the ratchet results
     (gv-diff e2e and the engine ratchets). For other changes: how you tested it. -->

## Changelog entry

<!-- The release copies this section into CHANGELOG.md; don't edit CHANGELOG.md yourself.
     One Keep a Changelog bullet per change a user would notice, output changes included, ending
     with the issue numbers, for example:
     - dot: a NaN spline no longer prints `Unable to reclaim box space` (#396).
     Use `### Fixed`, `### Changed`, `### Breaking` or `### Deprecated` headings for more than one
     kind, and add the `breaking` label for a breaking change. Nothing to say: write
     "Not user-visible." -->

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] No file that matched C stops matching; baselines refreshed if more now match
- [ ] `## Changelog entry` above filled in for anything a user would notice, including output changes
- [ ] New dependencies, if any, pass `cargo deny check`
