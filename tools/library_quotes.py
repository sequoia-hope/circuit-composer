#!/usr/bin/env python3
"""Check that each claim's quoted passage is in the document it cites: the text of every
cached PDF (tools/library_fetch.py --text) is searched for the quote, ignoring case, spacing,
hyphenation at line ends and typographic punctuation. Each page is tried in both pdftotext
modes (with -layout, and plain for two-column pages); a quote with "..." elisions must have
every piece on the page. A description of a figure goes in `figure`, not `quote`, and is not
searched. A quote not found whole is tried as its words in order with gaps (a column of
another text interleaved), and reported `scattered` for a reader to confirm. Reports the PDF page each quote was found on, so a locator can be compared with it.

    library_quotes.py [--root .] [--cache library/cache] [--claims FILE ...]

Prints one JSON result; exits 1 when a quote is not found in its source.
"""
import argparse
import glob
import json
import os
import re
import subprocess
import sys


def norm(s, join=True):
    """Lower-case words. A hyphen at a line end either splits a word (join) or is a real one."""
    s = s.replace("­", "")
    s = re.sub(r"-\s*\n\s*", "" if join else "-", s)
    for a, b in [("‘", "'"), ("’", "'"), ("“", '"'), ("”", '"'), ("–", "-"), ("—", "-"), ("−", "-"), ("ﬁ", "fi"), ("ﬂ", "fl"), ("µ", "u"), ("μ", "u"), ("Ω", "ohm"), ("Ω", "ohm")]:
        s = s.replace(a, b)
    return re.sub(r"[^a-z0-9]+", " ", s.lower()).strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    ap.add_argument("--cache", default=None)
    ap.add_argument("--claims", nargs="*")
    a = ap.parse_args()
    cache = a.cache or os.path.join(a.root, "library/cache")
    files = a.claims or sorted(glob.glob(os.path.join(a.root, "library/claims/*.json"))) + sorted(glob.glob(os.path.join(a.root, "packages/*/rules.json")))
    pages = {}

    def text(sid):
        """Each page's text in both modes, normalised."""
        if sid not in pages:
            base = sid.rsplit("/", 1)[-1]  # rp2350/rp2350_hdg: a package's source, cached by its id
            pdf, txt = os.path.join(cache, base + ".pdf"), os.path.join(cache, base + ".txt")
            if os.path.exists(pdf):
                modes = [subprocess.run(["pdftotext", *flag, pdf, "-"], capture_output=True, text=True, errors="replace").stdout.split("\f") for flag in (["-layout"], [])]
            elif os.path.exists(txt):
                # A web page, saved as text by library_fetch.py: one page.
                modes = [[open(txt, errors="replace").read()]] * 2
            else:
                modes = None
            pages[sid] = [" | ".join(norm(m, j) for m in (a, b) for j in (True, False)) for a, b in zip(*modes)] if modes else None
        return pages[sid]

    def on(page, q):
        return all(norm(piece) in page for piece in re.split(r"\.\.\.|\u2026", q) if norm(piece))

    def scattered(page, q, gap=60):
        """The quote's words in order on the page, each within `gap` words of the last."""
        words, want = page.split(), norm(q).split()
        if not want:
            return False
        for start in (i for i, w in enumerate(words) if w == want[0]):
            at, ok = start, True
            for w in want[1:]:
                nxt = next((j for j in range(at + 1, min(at + 1 + gap, len(words))) if words[j] == w), None)
                if nxt is None:
                    ok = False
                    break
                at = nxt
            if ok:
                return True
        return False

    results, counts = [], {}
    for f in files:
        for c in json.load(open(f))["claims"]:
            for cit in ([c["citation"]] if "citation" in c else []) + c.get("citations", []):
                q = cit.get("quote")
                if not q:
                    continue
                r = {"claim": c["id"], "source": cit["source"], "locator": cit.get("locator")}
                t = text(cit["source"])
                if t is None:
                    r["status"] = "no_text"
                else:
                    hit = [i + 1 for i, pg in enumerate(t) if on(pg, q)]
                    if not hit:
                        # A quote that crosses a page break.
                        hit = [i + 1 for i in range(len(t) - 1) if on(t[i] + " " + t[i + 1], q)]
                    if hit:
                        r.update(status="found", pdf_pages=hit)
                    else:
                        # Each mode on its own: the halves joined by " | " would let a quote run between them.
                        hit = [i + 1 for i, pg in enumerate(t) if any(scattered(m, q) for m in pg.split(" | "))]
                        if not hit:
                            hit = [i + 1 for i in range(len(t) - 1) if any(scattered(a + " " + b, q) for a, b in zip(t[i].split(" | "), t[i + 1].split(" | ")))]
                        r.update(status="scattered", pdf_pages=hit, quote=q) if hit else r.update(status="missing", quote=q)
                counts[r["status"]] = counts.get(r["status"], 0) + 1
                results.append(r)
    ok = counts.get("missing", 0) == 0
    print(json.dumps({"ok": ok, "counts": counts, "quotes": [r for r in results if r["status"] != "found"], "found": [r for r in results if r["status"] == "found"]}, indent=1))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
