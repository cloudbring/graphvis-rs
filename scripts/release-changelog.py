#!/usr/bin/env python3
"""Collects the changelog entries of the pull requests merged in a range of master.

Every pull request carries its entry in its body, under a `## Changelog entry` heading
(.github/pull_request_template.md), not in CHANGELOG.md. This reads the pull request number
(the last `(#N)` of each squash-merged commit's subject) for every commit in BASE..HEAD, fetches
the bodies and labels with `gh` (reads only), and sorts each entry's bullets into the CHANGELOG's sections.

An entry's bullets go under the `### Section` heading they follow in the body (Breaking, Added,
Changed, Fixed, Deprecated, Removed, Security) or the section their own bold prefix names
(`**Changed (breaking):** ...` is Breaking). Other bullets go under Breaking if the pull request
has the `breaking` label and no bullet of its own is marked Breaking, else Changed if they start
with "Output change", Added if they start with "Added", and Fixed otherwise. A bullet that names
no `#N` gets the pull request's number. An entry with no bullets that says it is not
user-visible ("Not user-visible", "None", "n/a") counts as an entry with nothing to list; an
absent or empty section is missing. A Breaking bullet in a pull request without the `breaking`
label is reported as `unlabelled_breaking`: the label decides the version.

    release-changelog.py collect BASE HEAD [--exclude N]...     JSON on stdout
    release-changelog.py render JSON VERSION [--intro TEXT]     the CHANGELOG section
    release-changelog.py next-version JSON PREVIOUS             the version the labels ask for

The version rule (docs/versioning.md): while the version is 0.x, a pull request labelled
`breaking` bumps the minor version and everything else the patch version; from 1.0 `breaking`
bumps the major version. Python standard library only (3.9).
"""
import datetime
import json
import re
import subprocess
import sys
import textwrap

REPO = "graphviz-pure/graphviz-pure"
BREAKING_LABEL = "breaking"
# The CHANGELOG's sections, in the order they are written.
SECTIONS = ["Breaking", "Added", "Changed", "Fixed", "Deprecated", "Removed", "Security"]
# The line release-prep.sh writes where the release's intro goes; release-ready.sh refuses it.
INTRO_PLACEHOLDER = "<!-- release-prep: replace this line with the release's intro paragraph -->"
NOT_VISIBLE = re.compile(r"^\W*(not user[- ]visible|no user[- ]visible|none|n/?a|nothing)\b", re.I)
WIDTH = 96


def commits(base, head):
    out = subprocess.check_output(["git", "log", "--first-parent", "--format=%H%x00%s", f"{base}..{head}"], text=True)
    result = []
    for line in out.splitlines():
        sha, subject = line.split("\0", 1)
        m = re.search(r"\(#(\d+)\)\s*$", subject)
        result.append({"sha": sha, "subject": subject, "pr": int(m.group(1)) if m else None})
    return result


def fetch_prs(numbers):
    """Body, title and labels of each pull request, through GraphQL, 25 at a time."""
    owner, name = REPO.split("/")
    prs = {}
    numbers = sorted(set(numbers))
    for i in range(0, len(numbers), 25):
        chunk = numbers[i:i + 25]
        fields = " ".join(
            f"p{n}: pullRequest(number: {n}) {{ number title url body labels(first: 30) {{ nodes {{ name }} }} }}"
            for n in chunk)
        query = f'query {{ repository(owner: "{owner}", name: "{name}") {{ {fields} }} }}'
        out = subprocess.check_output(["gh", "api", "graphql", "-f", f"query={query}"], text=True)
        repo = json.loads(out)["data"]["repository"]
        for n in chunk:
            pr = repo.get(f"p{n}")
            if pr:
                prs[n] = {"title": pr["title"], "url": pr["url"], "body": pr["body"] or "",
                          "labels": [label["name"] for label in pr["labels"]["nodes"]]}
    return prs


def section(body):
    """The text under `## Changelog entry`, or None if the body has no such heading."""
    body = re.sub(r"<!--.*?-->", "", body.replace("\r\n", "\n"), flags=re.S)
    lines, out, inside = body.split("\n"), [], False
    for line in lines:
        if re.match(r"^##\s+Changelog entry\s*$", line, re.I):
            inside = True
            continue
        if inside and (re.match(r"^#{1,2}\s", line) or line.startswith("🤖 Generated")):
            break
        if inside:
            out.append(line)
    return "\n".join(out) if inside else None


def parse(text, default):
    """The entry's bullets as (section, text) pairs, and its prose paragraphs."""
    bullets, prose, current, heading = [], [], None, None
    for raw in text.split("\n"):
        line = raw.rstrip()
        if line.strip().startswith("```"):
            continue
        # `### Fixed`, or a line of its own such as `Under **Fixed**:` or `**Changed**:`.
        # `Under **Changed** (breaking):` is a Breaking heading.
        m = re.match(r"^(?:#{3,}\s+(\w+)(.*)|(?:Under\s+)?\*\*(\w+)\*\*([^*]*))$", line.strip())
        if m and (m.group(1) or m.group(3)).capitalize() in SECTIONS:
            heading = (m.group(1) or m.group(3)).capitalize()
            if "breaking" in (m.group(2) or m.group(4) or "").lower():
                heading = "Breaking"
            current = None
            continue
        m = re.match(r"^\s{0,3}[-*]\s+(.*)$", line)
        if m:
            text, section = m.group(1).strip(), heading
            # A bullet that names its section: `**Changed (breaking):** ...`, `**Breaking:** ...`.
            p = re.match(r"^\*\*(\w+)([^*]*?):?\*\*:?\s+(.*)$", text)
            if p and p.group(1).capitalize() in SECTIONS:
                section = "Breaking" if "breaking" in (p.group(1) + p.group(2)).lower() else p.group(1).capitalize()
                text = p.group(3)
            current = [section, text]
            bullets.append(current)
        elif line.strip() and current is not None and (raw.startswith(" ") or raw.startswith("\t")):
            current[1] += " " + line.strip()
        elif line.strip():
            current = None
            if prose and prose[-1] is not None:
                prose[-1] += " " + line.strip()
            else:
                prose.append(line.strip())
        else:
            current = None
            if prose and prose[-1] is not None:
                prose.append(None)
    prose = [p for p in prose if p]
    result = []
    for heading, body in bullets:
        if heading is None:
            if default:
                heading = default
            elif body.lower().startswith("output change"):
                heading = "Changed"
            else:
                heading = "Added" if body.startswith("Added ") else "Fixed"
        result.append((heading, body))
    return result, prose


def collect(base, head, exclude):
    entries, missing, none, notes, without_pr = [], [], [], [], []
    rows = commits(base, head)
    prs = fetch_prs([c["pr"] for c in rows if c["pr"] and c["pr"] not in exclude])
    breaking, unlabelled = [], []
    for c in reversed(rows):  # oldest first, as the changes landed
        n = c["pr"]
        if n in exclude:
            continue
        if n is None or n not in prs:
            without_pr.append({"sha": c["sha"], "subject": c["subject"]})
            continue
        pr = prs[n]
        is_breaking = BREAKING_LABEL in pr["labels"]
        if is_breaking:
            breaking.append(n)
        text = section(pr["body"])
        info = {"pr": n, "title": pr["title"], "url": pr["url"]}
        if text is None or not text.strip():
            missing.append(info)
            continue
        bullets, prose = parse(text, None)
        if is_breaking and not any(h == "Breaking" for h, _ in bullets):
            bullets, prose = parse(text, "Breaking")
        if not bullets:
            if prose and NOT_VISIBLE.match(prose[0]):
                none.append(dict(info, reason=prose[0]))
                continue
            if not prose:
                missing.append(info)
                continue
            # A paragraph with no bullet is the entry itself.
            bullets = [("Breaking" if is_breaking else "Fixed", " ".join(prose))]
            prose = []
        if not is_breaking and any(h == "Breaking" for h, _ in bullets):
            unlabelled.append(info)
        for heading, body in bullets:
            if not re.search(r"#\d+", body):
                body = re.sub(r"\.?$", f" (#{n}).", body, count=1)
            entries.append({"pr": n, "section": heading, "text": body})
        if prose:
            notes.append(dict(info, notes=prose))
    return {"base": base, "head": head, "entries": entries, "missing": missing, "none": none,
            "notes": notes, "breaking": sorted(breaking), "unlabelled_breaking": unlabelled,
            "without_pr": without_pr}


def render(data, version, intro):
    out = [f"## [{version}] - {datetime.date.today().isoformat()}", "", intro or INTRO_PLACEHOLDER]
    for name in SECTIONS:
        items = [e["text"] for e in data["entries"] if e["section"] == name]
        if not items:
            continue
        out += ["", f"### {name}", ""]
        for text in items:
            out += textwrap.wrap(text, WIDTH, initial_indent="- ", subsequent_indent="  ",
                                 break_long_words=False, break_on_hyphens=False)
    return "\n".join(out) + "\n"


def next_version(data, previous):
    major, minor, patch = (int(x) for x in re.match(r"^(\d+)\.(\d+)\.(\d+)", previous).groups())
    if not data["breaking"]:
        return f"{major}.{minor}.{patch + 1}"
    return f"0.{minor + 1}.0" if major == 0 else f"{major + 1}.0.0"


def main(argv):
    if len(argv) >= 3 and argv[0] == "collect":
        exclude = {int(argv[i + 1]) for i, a in enumerate(argv) if a == "--exclude"}
        json.dump(collect(argv[1], argv[2], exclude), sys.stdout, indent=1)
        print()
    elif len(argv) >= 3 and argv[0] == "render":
        intro = argv[argv.index("--intro") + 1] if "--intro" in argv else None
        sys.stdout.write(render(json.load(open(argv[1])), argv[2], intro))
    elif len(argv) == 3 and argv[0] == "next-version":
        print(next_version(json.load(open(argv[1])), argv[2]))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
