#!/usr/bin/env python3
"""Import an extraction (a topic's proposed sources and claims) into the research library:
sources join library/sources.json, claims become library/claims/TOPIC.json. A source id
already in the library must name the same document (same sha256); a claim id already used
elsewhere is refused. Empty fields are dropped, and the files are written in the library's
compact style. A quote in parentheses is moved to `figure`. One key per line for each claim, short values inline.

    library_import.py EXTRACTION.json [--root .] [--note "..."] [--replace]

Run `ccomp library check` and tools/library_quotes.py afterwards.
"""
import argparse
import glob
import json
import os
import sys

W = 110


def compact(v):
    if isinstance(v, dict):
        return "{}" if not v else "{ " + ", ".join(f"{json.dumps(k, ensure_ascii=False)}: {compact(x)}" for k, x in v.items()) + " }"
    if isinstance(v, list):
        return "[" + ", ".join(compact(x) for x in v) + "]"
    return json.dumps(v, ensure_ascii=False)


def fmt(v, ind, depth):
    """Expanded down to `depth` levels, then inline where it fits in W columns."""
    c = compact(v)
    if not isinstance(v, (dict, list)) or not v or (depth <= 0 and len(c) + ind <= W):
        return c
    pad = " " * (ind + 2)
    if isinstance(v, dict):
        return "{\n" + ",\n".join(f"{pad}{json.dumps(k, ensure_ascii=False)}: {fmt(x, ind + 2, depth - 1)}" for k, x in v.items()) + "\n" + " " * ind + "}"
    return "[\n" + ",\n".join(pad + fmt(x, ind + 2, depth - 1) for x in v) + "\n" + " " * ind + "]"


def write(path, v, depth):
    open(path, "w").write(fmt(v, 0, depth) + "\n")


def clean(c):
    for cit in c.get("citations", []):
        # "(Figure 3 shows ...)" describes a figure; it is not text to search for.
        q = cit.get("quote", "")
        if q.startswith("(") and q.endswith(")"):
            cit["figure"] = cit.pop("quote")[1:-1]
    return {k: v for k, v in c.items() if v not in ([], {}, None, "")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("extraction")
    ap.add_argument("--root", default=".")
    ap.add_argument("--note", default="")
    ap.add_argument("--replace", action="store_true", help="overwrite an existing topic file")
    a = ap.parse_args()
    x = json.load(open(a.extraction))
    topic = x["topic"]
    lib = os.path.join(a.root, "library")
    out = os.path.join(lib, "claims", topic + ".json")
    if os.path.exists(out) and not a.replace:
        sys.exit(f"{out} exists; pass --replace")
    src_path = os.path.join(lib, "sources.json")
    sources = json.load(open(src_path))
    for sid, s in x["sources"].items():
        old = sources.get(sid)
        if old and old.get("sha256") != s.get("sha256"):
            sys.exit(f"source {sid} is already in the library as a different document")
        sources[sid] = clean(s)
    taken = {}
    for f in glob.glob(os.path.join(lib, "claims/*.json")) + glob.glob(os.path.join(a.root, "packages/*/rules.json")):
        if os.path.abspath(f) == os.path.abspath(out):
            continue
        for c in json.load(open(f))["claims"]:
            taken[c["id"]] = f
    for c in x["claims"]:
        if c["id"] in taken:
            sys.exit(f"claim {c['id']} is already in {taken[c['id']]}")
    doc = {"topic": topic}
    if a.note or x.get("note"):
        doc["note"] = a.note or x["note"]
    doc["claims"] = [clean(c) for c in x["claims"]]
    write(out, doc, 3)
    write(src_path, dict(sorted(sources.items())), 1)
    print(json.dumps({"topic": topic, "claims": len(doc["claims"]), "sources": len(x["sources"]), "file": out}))


if __name__ == "__main__":
    main()
