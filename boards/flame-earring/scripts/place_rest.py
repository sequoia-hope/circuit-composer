#!/usr/bin/env python3
"""A second floorplan pass for parts an earlier step left unplaced (bypass capacitors the rails
had no room for): everything already on the board is kept; every via is a back-side keep-out.

    place_rest.py BOARD REFS... -o SPEC.json
"""
import argparse
import json
from pathlib import Path

import pcbnew

BOARD_DIR = Path(__file__).resolve().parent.parent


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("refs", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    a = ap.parse_args()
    b = pcbnew.LoadBoard(a.board)
    spec = json.load(open(BOARD_DIR / "floorplan/spec.json"))
    refs = [f.GetReference() for f in b.GetFootprints()]
    spec["keep"] = sorted(r for r in refs if r not in a.refs and r != "H1") + ["H1"]
    spec["exclude"] = []
    vias = [t for t in b.GetTracks() if t.Type() == pcbnew.PCB_VIA_T]
    spec["keepouts"] = spec["keepouts"] + [{"name": f"via{i}", "rect": [v.GetPosition().x / 1e6 - 0.34, v.GetPosition().y / 1e6 - 0.34, v.GetPosition().x / 1e6 + 0.34, v.GetPosition().y / 1e6 + 0.34], "side": "back"} for i, v in enumerate(vias)]
    spec["options"]["moves"] = 200000
    spec["note"] = "Second pass (scripts/place_rest.py): only " + ", ".join(a.refs) + " move."
    json.dump(spec, open(a.output, "w"), indent=1)


if __name__ == "__main__":
    main()
