## What and why

<!-- What does this change, and which C function or file does it follow (lib/.../file.c:function)? -->

## Evidence

<!-- For a fidelity fix: the input that showed the difference, and the ratchet results
     (gv-diff e2e and the engine ratchets). For other changes: how you tested it. -->

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] No file that matched C stops matching; baselines refreshed if more now match
- [ ] CHANGELOG.md "Unreleased" updated for anything a user would notice, including output changes
- [ ] New dependencies, if any, pass `cargo deny check`
