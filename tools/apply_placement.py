#!/usr/bin/env python3
"""Apply a floorplan (`ccomp floorplan -o placement.json`) to a KiCad board with pcbnew: each
footprint's position and rotation, the bottom-side parts flipped, and the board outline drawn
on Edge.Cuts (replacing any outline there). Then KiCad's DRC is run on the result and its
courtyard findings counted.

    apply_placement.py BOARD PLACEMENT.json -o OUT.kicad_pcb [--back REF ...]

Prints one JSON result.
"""
import argparse
import json
import subprocess
import tempfile

import pcbnew

NM = 1e6


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("placement")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--back", nargs="*", default=[])
    a = ap.parse_args()
    pl = json.load(open(a.placement))
    b = pcbnew.LoadBoard(a.board)
    fps = {f.GetReference(): f for f in b.GetFootprints()}
    missing = [r for r in pl["parts"] if r not in fps]
    for r, p in pl["parts"].items():
        fp = fps.get(r)
        if fp is None:
            continue
        if (r in a.back) != fp.IsFlipped():
            fp.Flip(fp.GetPosition(), pcbnew.FLIP_DIRECTION_TOP_BOTTOM)
        fp.SetPosition(pcbnew.VECTOR2I(int(round(p["x"] * NM)), int(round(p["y"] * NM))))
        fp.SetOrientationDegrees(p["rotation"])
    for d in list(b.GetDrawings()):
        if d.GetLayer() == pcbnew.Edge_Cuts:
            b.Remove(d)
    x0, y0, x1, y1 = pl["outline"]
    rect = pcbnew.PCB_SHAPE(b)
    rect.SetShape(pcbnew.SHAPE_T_RECT)
    rect.SetStart(pcbnew.VECTOR2I(int(x0 * NM), int(y0 * NM)))
    rect.SetEnd(pcbnew.VECTOR2I(int(x1 * NM), int(y1 * NM)))
    rect.SetLayer(pcbnew.Edge_Cuts)
    rect.SetWidth(int(0.05 * NM))
    b.Add(rect)
    b.Save(a.output)
    with tempfile.NamedTemporaryFile(suffix=".json") as t:
        subprocess.run(["kicad-cli", "pcb", "drc", "--format", "json", "--severity-all", "-o", t.name, a.output], capture_output=True)
        drc = json.load(open(t.name))
    counts = {}
    for v in drc.get("violations", []):
        counts[v["type"]] = counts.get(v["type"], 0) + 1
    court = {k: v for k, v in counts.items() if "courtyard" in k or "edge" in k or "silk" not in k and "clearance" in k}
    print(json.dumps({"board": a.output, "moved": len(pl["parts"]) - len(missing), "missing": missing, "outline_mm": [round(x1 - x0, 2), round(y1 - y0, 2)], "drc_placement": court, "drc_all": counts, "unconnected": len(drc.get("unconnected_items", []))}, indent=1))


if __name__ == "__main__":
    main()
