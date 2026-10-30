#!/usr/bin/env python3
"""Download the research library's sources into a local cache and check them against their
recorded SHA-256, so every page a claim cites can be opened again. The documents themselves
are not in the repository (they are the vendors'); the hashes say which revision was read.

    library_fetch.py [--root .] [--cache library/cache] [--only ID ...] [--text]

Sources come from library/sources.json and every packages/*/sources/sources.json. A PDF is
kept as it came; a web page (an article) is kept as HTML and as its text, and since pages carry
ads and dates its hash is expected to drift: a changed web page is reported, not failed. A source
whose url is a zip names the PDF inside it under `member`; the hash is the PDF's. --text also writes pdftotext
output beside each PDF, one form feed per page, for searching a locator. Prints one JSON
result; exits 1 when a hash differs (the vendor has published a new revision: re-read it
and update the claims that cite it).
"""
import argparse
import glob
import hashlib
import html
import json
import os
import re
import subprocess
import sys
import urllib.request
import zipfile

UA = "Mozilla/5.0 (X11; Linux x86_64) circuit-composer library_fetch"


def sources(root):
    out = {}
    for path in [os.path.join(root, "library/sources.json")] + sorted(glob.glob(os.path.join(root, "packages/*/sources/sources.json"))):
        for sid, s in json.load(open(path)).items():
            out[sid] = (os.path.relpath(path, root), s)
    return out


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    ap.add_argument("--cache", default=None, help="default ROOT/library/cache (git-ignored)")
    ap.add_argument("--only", nargs="*")
    ap.add_argument("--text", action="store_true")
    a = ap.parse_args()
    cache = a.cache or os.path.join(a.root, "library/cache")
    os.makedirs(cache, exist_ok=True)
    results, ok = [], True
    for sid, (owner, s) in sorted(sources(a.root).items()):
        if a.only and sid not in a.only:
            continue
        url = s.get("url", "")
        if not url.startswith("http"):
            results.append({"source": sid, "status": "skipped", "why": "no url"})
            continue
        web = s.get("kind") == "article" and not url.lower().split("?")[0].endswith(".pdf")
        if not web and not s.get("member") and not url.lower().split("?")[0].endswith(".pdf") and s.get("kind") not in ("datasheet", "app_note", "design_guide", "standard"):
            results.append({"source": sid, "status": "skipped", "why": "not a document"})
            continue
        dest = os.path.join(cache, sid + (".html" if web else ".pdf"))
        r = {"source": sid, "from": owner, "file": dest}
        if not os.path.exists(dest):
            try:
                req = urllib.request.Request(url, headers={"User-Agent": UA})
                with urllib.request.urlopen(req, timeout=180) as resp, open(dest + ".part", "wb") as f:
                    f.write(resp.read())
                if s.get("member"):
                    with zipfile.ZipFile(dest + ".part") as z, open(dest + ".member", "wb") as f:
                        f.write(z.read(s["member"]))
                    os.replace(dest + ".member", dest + ".part")
                os.replace(dest + ".part", dest)
            except Exception as e:
                r.update(status="unreachable", error=str(e))
                results.append(r)
                continue
        got = sha256(dest)
        want = s.get("sha256")
        if want is None:
            r.update(status="unhashed", sha256=got)
        elif got == want:
            r["status"] = "ok"
        elif web:
            r.update(status="web_changed", sha256=got, recorded=want)
        else:
            # Kept for reading; the claims citing it need re-checking against the new revision.
            r.update(status="changed", sha256=got, recorded=want)
            ok = False
        if web:
            page = open(dest, errors="replace").read()
            page = re.sub(r"(?is)<(script|style)\b.*?</\1>", " ", page)
            page = re.sub(r"(?i)<(br|/p|/h\d|/li|/div)\b[^>]*>", "\n", page)
            open(dest[:-5] + ".txt", "w").write(html.unescape(re.sub(r"<[^>]+>", " ", page)))
        elif a.text:
            subprocess.run(["pdftotext", "-layout", dest, dest[:-4] + ".txt"], check=False)
        results.append(r)
    counts = {}
    for r in results:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    print(json.dumps({"ok": ok, "counts": counts, "sources": results}, indent=1))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
