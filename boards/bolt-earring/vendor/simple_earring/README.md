# simple_earring, vendored

`scripts/circuit.py`, `bom/parts.yaml` and `library/parts.pretty/` from
`~/pcb/simple_earring` at commit 7a83b33 ("Route with ultraroute; widen the LED supply tracks to
0.3 mm"), copied unchanged. The flame earring reuses that board's circuit: every part except
the LED field, whose count, zones and chain order come from this board's geometry
(`../../scripts/circuit.py` replaces `led_zones`). A pinned copy, so later work on
simple_earring does not change this board without a decision.
