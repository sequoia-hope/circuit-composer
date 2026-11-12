#!/usr/bin/env python3
"""Score a board by comma.ai's PCBGolf formula: PCBA bounding-box volume (mm³) + 50 × vias +
5,000 × copper layers.

    pcbgolf_score.py BOARD [--kicad-cli CMD] [--keep DIR]

The volume is measured the way the assembly is judged: KiCad exports the assembled board (board
and every 3D model) to STL, and the box is taken over its triangles. Vias and copper layers are
counted with pcbnew. Both run in KiCad 10 (the Flatpak by default, since the PCBGolf files are
KiCad 10). The board must live under $HOME: the Flatpak cannot see /tmp. Prints one JSON object.
"""
import argparse
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile

FLATPAK = ["flatpak", "run", "--command={}", "org.kicad.KiCad"]


def kicad(tool, args):
    cmd = [part.format(tool) for part in FLATPAK] + args
    return subprocess.run(cmd, capture_output=True, text=True)


def stl_bbox(path):
    data = open(path, "rb").read()
    lo, hi = [float("inf")] * 3, [float("-inf")] * 3
    if data[:5] == b"solid" and b"facet" in data[:400]:
        for line in data.decode(errors="ignore").splitlines():
            parts = line.split()
            if parts and parts[0] == "vertex":
                v = list(map(float, parts[1:4]))
                lo = [min(a, b) for a, b in zip(lo, v)]
                hi = [max(a, b) for a, b in zip(hi, v)]
    else:
        (n,) = struct.unpack_from("<I", data, 80)
        for i in range(n):
            off = 84 + i * 50 + 12
            for k in range(3):
                v = struct.unpack_from("<3f", data, off + 12 * k)
                lo = [min(a, b) for a, b in zip(lo, v)]
                hi = [max(a, b) for a, b in zip(hi, v)]
    return lo, hi


COUNT = """
import pcbnew, sys, json
b = pcbnew.LoadBoard(sys.argv[1])
vias = sum(1 for t in b.Tracks() if t.GetClass() == 'PCB_VIA')
print(json.dumps({'vias': vias, 'copper_layers': b.GetCopperLayerCount(),
                  'thickness_mm': b.GetDesignSettings().GetBoardThickness() / 1e6,
                  'footprints': len(b.GetFootprints())}))
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("--keep", help="keep the exported STL in this directory")
    a = ap.parse_args()
    board = os.path.abspath(a.board)
    work = tempfile.mkdtemp(prefix=".pcbgolf-score-", dir=os.path.dirname(board))
    try:
        stl = os.path.join(work, "assembly.stl")
        r = kicad("kicad-cli", ["pcb", "export", "stl", "--subst-models", "--include-tracks", "--include-pads", "--include-zones", "-f", "-o", stl, board])
        if not os.path.exists(stl):
            print(json.dumps({"ok": False, "error": "STL export failed", "log": (r.stdout + r.stderr)[-2000:]}))
            sys.exit(2)
        lo, hi = stl_bbox(stl)
        size = [h - l for l, h in zip(lo, hi)]
        script = os.path.join(work, "count.py")
        open(script, "w").write(COUNT)
        c = kicad("python3", [script, board])
        counts = json.loads(c.stdout.strip().splitlines()[-1])
        volume = size[0] * size[1] * size[2]
        score = volume + 50 * counts["vias"] + 5000 * counts["copper_layers"]
        missing = [l for l in (r.stdout + r.stderr).splitlines() if "not found" in l.lower() or "could not" in l.lower()]
        out = {"ok": True, "score": round(score, 1), "volume_mm3": round(volume, 1), "bbox_mm": [round(s, 3) for s in size],
               "vias": counts["vias"], "copper_layers": counts["copper_layers"], "thickness_mm": counts["thickness_mm"],
               "footprints": counts["footprints"], "export_warnings": missing[:20]}
        if a.keep:
            os.makedirs(a.keep, exist_ok=True)
            shutil.copy(stl, os.path.join(a.keep, "assembly.stl"))
        print(json.dumps(out, indent=2))
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
