#!/usr/bin/env python3
"""Report what in a pull request needs a careful human read, without running any of it.

Reads only git objects (`git diff`, `git show`) for the change BASE..HEAD and prints a
Markdown report: sensitive files touched, new or changed dependencies, weakened ratchet
baselines, edited C references, and added lines that spawn processes, reach the network,
read the environment, loosen lints or look obfuscated. Nothing from the pull request is
built, imported or executed.

  python3 -I scripts/pr-risk.py BASE HEAD            # e.g. origin/master refs/pr/61
  python3 -I scripts/pr-risk.py HEAD^1 HEAD          # CI, on the pull_request merge commit

Exit status: 0 when nothing blocks; 1 when the change adds invisible or bidirectional
Unicode controls (Trojan Source) or a symlink; 2 on usage errors. Every other finding is
for the reviewer to judge and does not fail. CI runs the copy of this script from the base
branch, so a pull request cannot edit the scanner to hide itself.
"""
import json
import re
import subprocess
import sys

# Paths whose change can alter what CI, a release or a maintainer's machine executes, or
# what the project promises. Any change here gets a line-by-line read.
SENSITIVE = [
    (r"^\.github/", "GitHub configuration (workflows, CODEOWNERS, Dependabot)"),
    (r"(^|/)build\.rs$", "build script: runs on every `cargo build`, including yours"),
    (r"(^|/)Cargo\.toml$", "crate manifest (dependencies, features, build settings)"),
    (r"^Cargo\.lock$", "lockfile"),
    (r"^(deny\.toml|rust-toolchain(\.toml)?|\.cargo/)", "toolchain or supply-chain policy"),
    (r"^scripts/", "script run by CI, releases or maintainers"),
    (r"(^|/)(package(-lock)?\.json|\.npmrc|jsr\.json)$", "npm manifest or lockfile"),
    (r"^(LICENSE|NOTICE|SECURITY\.md|CODE_OF_CONDUCT\.md)$", "licence or policy document"),
    (r"^\.wtp\.yml$|(^|/)\.envrc$|(^|/)\.(vscode|idea|claude)/", "editor or agent config that can run commands"),
    (r"^crates/graphviz-pure-portable-libm/", "the only crate allowed `unsafe`"),
]

# C Graphviz's committed outputs: regenerating them needs the C oracle, so a contributor
# editing them by hand is changing the answer key.
GOLDEN_REF = re.compile(r"^tests/golden/.*\.(dot|svg|json0?|xdot|plain|canon|png)$")
BASELINE = re.compile(r"^tests/golden/.*\.json$")
PANIC_SITES = "tests/panic-sites.tsv"

# Added-line patterns, by file kind. Each is (regex, why it matters).
CODE_PATTERNS = [
    (r"\bCommand::new\b|\bsubprocess\b|\bchild_process\b|\bos\.system\b|\bexecSync\b|\bspawn\(", "spawns a process"),
    (r"\bstd::net\b|\bTcpStream\b|\bUdpSocket\b|\breqwest\b|\bureq\b|\bhyper\b|\burllib\b|\bfetch\(|\bcurl\b|\bwget\b", "network access"),
    (r"\benv::var|\bstd::env\b|\bos\.environ\b|\bprocess\.env\b|\bsecrets\.|_TOKEN\b|\.ssh/|\.aws/|\.npmrc", "reads the environment or credentials"),
    (r"\binclude_(bytes|str)!|\bremove_dir_all\b|\brmtree\b|\brm -rf\b", "embeds or deletes files"),
    (r"\bunsafe\b|unsafe_code|#!?\[allow\(|\bextern \"C\"|\bno_mangle\b|\blink_section\b", "unsafe code or a lint exemption"),
    (r"\beval\(|\bexec\(|\bnew Function\(|\bbase64\b|\bfrom_base64\b|\batob\(", "dynamic evaluation or decoding"),
    (r"\bproc_macro\b|\bproc-macro\b", "procedural macro: runs inside the compiler"),
]
OBFUSCATED = [
    (re.compile(r"[A-Za-z0-9+/=]{120,}"), "long base64-like blob"),
    (re.compile(r"(\\x[0-9a-fA-F]{2}){12,}"), "long run of hex escapes"),
    (re.compile(r"(0x[0-9a-fA-F]{2},\s*){24,}"), "long byte array"),
]
CODE_EXT = re.compile(r"\.(rs|py|sh|mjs|cjs|js|ts|toml|ya?ml|json)$")
# Removing one of these weakens a guard.
GUARDS = re.compile(r"unsafe_code|\[lints|lints\.workspace|forbid\(|deny\(|persist-credentials: false|permissions:|--locked|#\[test\]|assert")

# Trojan Source (CVE-2021-42574) and invisible characters. A BOM at the start of a file is fine.
INVISIBLE = re.compile("[\u202a-\u202e\u2066-\u2069\u200b-\u200f\u2060-\u2064\u00ad\ufeff]")


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True).stdout


def show(rev, path):
    try:
        return git("show", f"{rev}:{path}")
    except subprocess.CalledProcessError:
        return None


def lock_packages(text):
    """name -> set of (version, source) from a Cargo.lock; no TOML parser needed."""
    out = {}
    for block in (text or "").split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        ver = re.search(r'^version = "([^"]+)"', block, re.M)
        src = re.search(r'^source = "([^"]+)"', block, re.M)
        if name and ver:
            out.setdefault(name.group(1), set()).add((ver.group(1), src.group(1) if src else "local"))
    return out


def npm_packages(text):
    try:
        data = json.loads(text or "{}")
    except ValueError:
        return {}
    pkgs = data.get("packages", {})
    return {k: (v.get("version"), v.get("resolved"), bool(v.get("hasInstallScript"))) for k, v in pkgs.items() if k}


def baseline_weakening(old, new, path=""):
    """Places where a ratchet baseline now expects less than before."""
    found = []
    if isinstance(old, dict) and isinstance(new, dict):
        for k, v in old.items():
            p = f"{path}/{k}"
            if k not in new:
                found.append(f"`{p}` removed")
            elif isinstance(v, bool) and v and new[k] is False:
                found.append(f"`{p}` true → false")
            elif isinstance(v, (int, float)) and not isinstance(v, bool) and k.endswith("_ok") and new[k] < v:
                found.append(f"`{p}` {v} → {new[k]}")
            else:
                found += baseline_weakening(v, new[k], p)
    elif isinstance(old, list) and isinstance(new, list) and all(isinstance(x, str) for x in old):
        found += [f"`{path}` no longer lists `{x}`" for x in sorted(set(old) - set(new))]
    return found


def panic_sites_raised(old, new):
    """Counts in the panic-site baseline (scripts/panic-sites.sh) that went up."""
    def rows(text):
        lines = text.splitlines()
        if not lines:
            return [], {}
        kinds = lines[0].split("\t")[1:]
        return kinds, {f[0]: f[1:] for f in (l.split("\t") for l in lines[1:]) if f}
    _, before = rows(old)
    kinds, after = rows(new)
    found = []
    for module, counts in after.items():
        for kind, b, a in zip(kinds, before.get(module, ["0"] * len(kinds)), counts):
            if a.isdigit() and b.isdigit() and int(a) > int(b):
                found.append(f"`{module}` {kind} {b} → {a}")
    return found


def main():
    if len(sys.argv) != 3:
        print(__doc__.split("\n\n")[2], file=sys.stderr)
        return 2
    base, head = sys.argv[1], sys.argv[2]
    blocking, notes = [], []
    section = lambda title, items: items and notes.append((title, items))  # noqa: E731

    # Files: status, mode and size.
    raw = git("diff", "--raw", "-z", "--no-renames", f"{base}", f"{head}").decode()
    fields = raw.split("\0")
    changes = []  # (status, path, new_mode)
    for i in range(0, len(fields) - 1, 2):
        meta, path = fields[i].split(), fields[i + 1]
        if len(meta) >= 5:
            changes.append((meta[4], path, meta[1]))
    numstat = {}
    for line in git("diff", "--numstat", "--no-renames", base, head).decode().splitlines():
        a, d, p = line.split("\t", 2)
        numstat[p] = (a, d)

    files = [f"`{s}` {p}  (+{numstat.get(p, ('?', '?'))[0]} −{numstat.get(p, ('?', '?'))[1]})" for s, p, _ in changes]
    section(f"Files changed ({len(changes)})", files)

    sens = []
    for s, p, _ in changes:
        for rx, why in SENSITIVE:
            if re.search(rx, p):
                sens.append(f"{p} — {why}")
                break
    section("Sensitive paths (read every line)", sens)

    links = [p for s, p, mode in changes if mode == "120000"]
    blocking += [f"adds a symlink: {p}" for p in links]
    execs = [p for s, p, mode in changes if mode == "100755" and not p.startswith("scripts/")]
    binaries = [p for p, (a, d) in numstat.items() if a == "-"]
    section("Executable files outside scripts/", execs)
    section("Binary files (cannot be reviewed as text)", binaries)

    goldens = [p for s, p, _ in changes if GOLDEN_REF.search(p) and not BASELINE.search(p)]
    section("C reference outputs edited (these come from the C oracle, not by hand)", goldens)

    weakened = []
    for s, p, _ in changes:
        if BASELINE.search(p) and s == "M":
            try:
                old, new = json.loads(show(base, p)), json.loads(show(head, p))
            except (TypeError, ValueError):
                continue
            weakened += [f"{p}: {w}" for w in baseline_weakening(old, new)]
    for s, p, _ in changes:
        if p == PANIC_SITES and s == "M":
            old, new = show(base, p) or b"", show(head, p) or b""
            weakened += [f"{p}: {w}" for w in panic_sites_raised(old.decode(), new.decode())]
    section("Ratchet baselines weakened (a file that matched C stops matching, or a panic site was added)", weakened[:40]
            + ([f"… and {len(weakened) - 40} more"] if len(weakened) > 40 else []))

    # Dependencies.
    if any(p == "Cargo.lock" for _, p, _ in changes):
        old, new = lock_packages((show(base, "Cargo.lock") or b"").decode()), lock_packages((show(head, "Cargo.lock") or b"").decode())
        deps = []
        for name in sorted(set(old) | set(new)):
            a, b = old.get(name, set()), new.get(name, set())
            for ver, src in sorted(b - a):
                kind = "new crate" if name not in old else "changed"
                odd = "" if src.startswith("registry+https://github.com/rust-lang/crates.io-index") or src == "local" else f" — source {src}"
                deps.append(f"{kind}: {name} {ver}{odd}")
            if name not in new:
                deps.append(f"removed: {name}")
        section("Cargo.lock changes", deps)
    for s, p, _ in changes:
        if p.endswith("package-lock.json"):
            old = npm_packages((show(base, p) or b"").decode())
            new = npm_packages((show(head, p) or b"").decode())
            deps = []
            for k in sorted(set(new) - set(old)):
                v, res, script = new[k]
                deps.append(f"new: {k} {v}" + (" — has an install script" if script else ""))
            for k in sorted(set(new) & set(old)):
                if new[k] != old[k]:
                    deps.append(f"changed: {k} {old[k][0]} → {new[k][0]}" + (" — has an install script" if new[k][2] else ""))
                    if new[k][1] and "registry.npmjs.org" not in new[k][1]:
                        deps.append(f"  resolved outside the npm registry: {new[k][1]}")
            section(f"{p} changes", deps)

    # Added and removed lines.
    diff = git("diff", "-U0", "--no-renames", "--no-color", base, head).decode("utf-8", "replace")
    flagged, removed_guards, invisible = {}, [], []
    path, lineno = None, 0
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
            continue
        if line.startswith("@@"):
            m = re.search(r"\+(\d+)", line)
            lineno = int(m.group(1)) if m else 0
            continue
        if path is None or line.startswith("--- "):
            continue
        if line.startswith("-") and CODE_EXT.search(path or "") and GUARDS.search(line):
            removed_guards.append(f"{path}: `{line[1:].strip()[:120]}`")
        if not line.startswith("+"):
            continue
        text = line[1:]
        n, lineno = lineno, lineno + 1
        here = f"{path}:{n}"
        m = INVISIBLE.search(text)
        if m and not (m.group() == "\ufeff" and n == 1 and m.start() == 0):
            invisible.append(f"{here}: U+{ord(m.group()):04X}")
        if not CODE_EXT.search(path):
            continue
        is_test_data = path.startswith("tests/") and not path.endswith(".rs")
        for rx, why in CODE_PATTERNS:
            if not is_test_data and re.search(rx, text):
                flagged.setdefault(why, []).append(f"{here}: `{text.strip()[:120]}`")
        for rx, why in OBFUSCATED:
            if rx.search(text):
                flagged.setdefault(why, []).append(f"{here}")
        if path.startswith((".github/", "scripts/")) or path.endswith("package.json"):
            for url in re.findall(r"https?://[^\s\"'`)>]+", text):
                if not re.match(r"https?://(github\.com/graphviz-pure|graphviz-pure\.github\.io|crates\.io|registry\.npmjs\.org)", url):
                    flagged.setdefault("new URL in CI, scripts or npm config", []).append(f"{here}: {url}")

    blocking += [f"invisible or bidirectional Unicode control at {x}" for x in invisible]
    for why, items in flagged.items():
        section(f"Added lines: {why}", items[:25] + ([f"… and {len(items) - 25} more"] if len(items) > 25 else []))
    section("Removed lines that held a guard (lint, permission, test or assertion)", removed_guards[:25])

    print(f"## PR risk report: `{base}`..`{head}`\n")
    if blocking:
        print("**Blocking:**\n")
        print("\n".join(f"- {b}" for b in blocking) + "\n")
    flagged_titles = [t for t, _ in notes if not t.startswith("Files changed")]
    if not blocking and not flagged_titles:
        print("Nothing flagged. Still read the diff: this report finds suspicious shapes, not intent.\n")
    for title, items in notes:
        print(f"### {title}\n")
        print("\n".join(f"- {x}" for x in items) + "\n")
    return 1 if blocking else 0


if __name__ == "__main__":
    sys.exit(main())
