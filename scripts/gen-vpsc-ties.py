#!/usr/bin/env python3
"""Writes tests/vpsc-ties/: inputs where VPSC's ties make C's layout depend on heap addresses.

`neato mode=ipsep overlap=vpsc` with clusters: chains with one-node or two-node clusters at
the start, middle or end (where a member can be one of initCMajVPSC's boundary variables,
#225), random digraphs with clusters, and #225's own input. The measurement over them is in
docs/porting/neato.md (VPSC, "Ties by address"); scripts/vpsc-ties-measure.py repeats it.
"""
import pathlib
import random

OUT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "vpsc-ties"
HEAD = "digraph { layout=neato; mode=ipsep; overlap=vpsc; "


def chain(n, k, pick):
    nodes = [f"n{i}" for i in range(n)]
    if pick == "last":
        cl = [[nodes[-1 - j]] for j in range(k)]
    elif pick == "first":
        cl = [[nodes[j]] for j in range(k)]
    else:
        cl = [[nodes[n // 2 + j], nodes[n // 2 + j + 2]] for j in range(k)]
    return graph(nodes, cl, [(nodes[i], nodes[i + 1]) for i in range(n - 1)])


def graph(nodes, clusters, edges):
    return (HEAD + " ".join(f"{x};" for x in nodes) + " "
            + " ".join(f"subgraph cluster_{j} {{ {' '.join(c)} }}" for j, c in enumerate(clusters))
            + " " + " ".join(f"{a} -> {b};" for a, b in edges) + " }\n")


def main():
    OUT.mkdir(exist_ok=True)
    for f in OUT.glob("*.gv"):
        f.unlink()
    for n in (8, 12, 20):
        for k in (1, 2):
            for pick in ("last", "first", "mid"):
                (OUT / f"chain-n{n}-k{k}-{pick}.gv").write_text(chain(n, k, pick))
    rng = random.Random(225)
    for i in range(22):
        n = rng.randint(5, 14)
        nodes = [f"v{j}" for j in range(n)]
        clusters, used = [], set()
        for _ in range(rng.randint(1, 3)):
            mem = [m for m in rng.sample(nodes, rng.randint(1, 3)) if m not in used]
            used |= set(mem)
            if mem:
                clusters.append(mem)
        edges = [tuple(rng.sample(nodes, 2)) for _ in range(rng.randint(n, 2 * n))]
        (OUT / f"random-{i:02}.gv").write_text(graph(nodes, clusters, edges))
    # #225's input: a 20-node chain whose last node is a one-node cluster.
    nodes = [f"n{i}" for i in range(20)]
    (OUT / "issue225.gv").write_text(
        HEAD + " ".join(f"{x};" for x in nodes) + " subgraph cluster_0 { n19 } "
        + " ".join(f"n{i} -> n{i + 1};" for i in range(19)) + " }\n")


if __name__ == "__main__":
    main()
