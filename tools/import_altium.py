#!/usr/bin/env python3
"""Import an Altium Designer `.PcbDoc` as a KiCad `.kicad_pcb` through pcbnew.

    import_altium.py BOARD.PcbDoc -o OUT.kicad_pcb [--refill]

What survives KiCad's Altium importer:
- copper layers, stackup thickness, Edge.Cuts outline,
- footprints with reference designator, Value, pads, pad numbers and pad nets,
- tracks, arcs, vias and copper pours / plane zones (pass `--refill` if zones arrive unfilled).

What is lost without the schematic or library:
- pin electrical types and pin function names (only pad numbers and net names sit on `.PcbDoc`),
- supplier / MPN parameters that Altium kept on the schematic symbol rather than the PCB comment,
- Altium-specific design rule expressions (rooms, query-scoped clearances, matched-length groups).

Prints one JSON summary on stdout.
"""
import argparse
import contextlib
import io
import json
import os
import sys


def main():
    ap = argparse.ArgumentParser(description="Import an Altium .PcbDoc into a KiCad .kicad_pcb.")
    ap.add_argument("input", help="Path to input .PcbDoc")
    ap.add_argument("-o", "--output", required=True, help="Output .kicad_pcb path")
    ap.add_argument("--refill", action="store_true", help="Recompute zone fills with pcbnew ZONE_FILLER")
    a = ap.parse_args()

    in_path = os.path.abspath(os.path.expanduser(a.input))
    out_path = os.path.abspath(os.path.expanduser(a.output))
    if not os.path.isfile(in_path):
        print(json.dumps({"ok": False, "error": f"not found: {in_path}"}))
        sys.exit(2)

    # Redirect pcbnew's C++ wxLog warnings (unmapped mechanical layers, font substitutions) off stdout
    # so stdout stays pure JSON.
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    saved_fd = os.dup(1)
    try:
        os.dup2(2, 1)
        import pcbnew

        sm = pcbnew.GetSettingsManager()
        sm.LoadProject("")
        board = pcbnew.LoadBoard(in_path, pcbnew.PCB_IO_MGR.ALTIUM_DESIGNER)
        if a.refill and board.GetAreaCount() > 0:
            pcbnew.ZONE_FILLER(board).Fill(board.Zones())
        pcbnew.SaveBoard(out_path, board)
        copper_layers = [
            board.GetLayerName(lid)
            for lid in board.GetEnabledLayers().CuStack()
        ]
        summary = {
            "ok": True,
            "input": in_path,
            "wrote": out_path,
            "refilled": a.refill,
            "copper_layers": copper_layers,
            "footprints": len(board.GetFootprints()),
            "tracks": len(board.GetTracks()),
            "zones": board.GetAreaCount(),
            "nets": board.GetNetCount(),
        }
    except Exception as exc:
        os.dup2(saved_fd, 1)
        os.close(saved_fd)
        print(json.dumps({"ok": False, "error": str(exc)}))
        sys.exit(2)

    os.dup2(saved_fd, 1)
    os.close(saved_fd)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
