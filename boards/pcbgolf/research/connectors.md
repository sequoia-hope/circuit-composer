# PCBGolf connector substitutes: lowest-profile parts that mate the original plugs

Compiled 2026-10-27. Stock and LCSC numbers come from the JLC/LCSC catalogue on that date. Every dimension carries a citation (datasheet URL plus page or figure) in the section tables below. Values marked *derived* or *listing only* are flagged where they appear.

## Summary: best choice per connector

| # | Connector | Original (height) | Best substitute | LCSC | Height | Notes |
|---|---|---|---|---|---|---|
| 1 | 4× OBD-C, 24-pin USB-C | GCT USB4115 vertical, 9.30 mm | XYECONN TYPE-C-G06-1BBWC0.8, mid-mount | C22385227 | 3.16 mm total (0.80 mm below the board) | All SS contacts populated. Other options: Hroparts TYPE-C-31-M-27 (C707318), hanxia HX-TYPE-C 24P-0.8/1.6. There is no stacked 24-pin part, so use two singles. |
| 2 | USB-C host | JAE DX07S024XJ1R1100, 3.34 mm | SHOU HAN TYPE-C 16P CB1.6 073, mid-mount | C2906290 | 3.16 mm total (1.60 mm below the board) | GCT USB4500/USB4510 are equivalents. |
| 3 | 12 V barrel jack | PJ-002AH-SMT-TR, 11 mm (Ø2.0 pin, mates 5.5/2.1 plug) | G-Switch DC-S007A-D020-15A, mid-mount | C20611871 | 6.85 mm total, about 5.9 mm above the board | Needs a 9.80×11.80 mm board-edge cutout. |
| 4 | microSD | Molex 47219-2001, 1.9 mm (listing only, Molex drawing not retrieved) | XUNPU TF-128-ARP9, push-pull | C5174689 | 1.15 mm | Lowest push-push: JAE ST11S008V4HR2000 (C3177044), 1.25 mm. No mid-mount part is lower. |
| 5 | CAN header 2×4 | 2.54 mm vertical male | HX PZ2.54-2x4P WZ-P, right-angle THT | C42453962 | ≤6.90 mm above the board | The plug mates sideways at the board edge. |
| 6 | SW1 tactile | Panasonic EVQ-Q2, 6.5×6.0×2.0–3.1 mm | TS3735A | C2906282 | 0.35 mm | Smallest footprint: KXT321LHS (C221821), 3.0×2.0×0.6 mm. |

JLC 1-layer boards: holes are NPTH only and the board must be at least 0.8 mm thick. Assembly on 1-layer boards is listed under Standard PCBA ("1–32 layers") but is not explicitly confirmed; see the last section. Economic PCBA requires 2, 4 or 6 layers at 0.8–1.6 mm, while Standard PCBA lets a 2-layer board go down to 0.4 mm.

## 1–2. USB Type-C receptacles (4× OBD-C full 24-pin ports, 1× host port)

Method: I pulled the JLCPCB parts catalogue through its parts-search API (`jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList`) on 2026-10-27. I collected every "USB Connectors" result for Type-C / 24P / 16P / 沉板 (sink) / USB4xxx, kept the female parts that were in stock, then downloaded each candidate's datasheet from LCSC or GCT and read the drawings as images. Stock counts are JLC/LCSC stock on 2026-10-27. All of these parts are **Extended** library ("expand"): JLC SMT-assembles them, with the extended-part loading fee. Every dimension below is quoted from the cited drawing. Anything marked *derived* is arithmetic on quoted drawing numbers, and the arithmetic is shown.

Terminology in LCSC/JLC descriptions:
- **"Recessed" / "Laminated board" / 沉板 ("sink board") / "CBx.x"** = mid-mount. The body drops into a board cutout and the SMT tails sit on the **top** copper. These suit a 1-layer board.
- **"Clamping plate" / 夹板 ("JB", "CP")** = straddle mount. The board edge is clamped between a top row and a bottom row of tails, so it needs pads on **both** sides. **Not usable on a 1-layer board.** Excluded (examples: HCTL HC-TYPE-C-24P-CP0.8/CP1.0, XYECONN TYPE-C-G09-1JBWJ, Kinghelm KH-TYPE-C-JB0.8-24P, CHUANGYUE …C0.8L8.8).

### Key physical fact: the floor for receptacle height is ~3.16 mm

Every mid-mount and top-mount 24-pin and 16-pin receptacle I found has a **3.16 mm** tall shell:
- USB-C opening 2.56 ±0.04 mm + 2 × 0.30 mm stainless shell = 3.16 mm.
- 3.16 mm is explicitly dimensioned on the Hroparts TYPE-C-31-M-27, XYECONN TYPE-C-G06-1BBWC0.8, GCT USB4500/USB4510, and SHOU HAN TYPE-C 16P CB1.6 drawings.
- I found no thinner standard receptacle.

For a mid-mount part, the sink depth only splits that 3.16 mm between above-board and below-board:
- **Assembled Z of the connector = max(board thickness, sink depth) + (3.16 − sink depth).**
- If sink depth ≥ board thickness, the connector sets the whole bounding-box height at exactly 3.16 mm.
- Matching sink depth to board thickness puts the board bottom flush with the shell bottom.
- A deeper sink (1.6 mm) on a 1.6 mm board leaves only 1.56 mm of shell above the top copper, which caps how tall every other top-side part can be.
- A shallow sink (0.8 mm) on a 0.8 mm board leaves 2.36 mm above.
- Either way the box is 3.16 mm.
- A top-mount part on a board of thickness t gives t + 3.16 mm or more.

### Originals (confirmed from datasheets)

| Original | Mount | Height | Body / footprint | Pins | Source |
|---|---|---|---|---|---|
| **GCT USB4115** (24-contact USB3.2 Gen2 vertical, "SMT, H=9.30mm"). It matches "USB-C-FEMALE-VERT-GCT, about 9 mm". USB4105 itself is a horizontal 16-pin USB 2.0 top-mount, not vertical. | Vertical, SMT plus pegs | **9.30 mm** above board; pegs 1.15 below | Body 8.94 × 3.16; 7.50 across pegs | 24 | https://gct.co/files/drawings/usb4115.pdf p.1 (front/side views; title block "SMT, H=9.30mm"). LCSC **C5250720**, stock 2016 |
| **JAE DX07S024XJ1R1100**. The brief calls it Hirose; it is a **JAE** part. | Horizontal top-mount, SMT + TH pegs | **3.34 mm** to top surface of bracket ("(3.34) TOP SURFACE OF BRACKET", mounting-condition view); center height 1.6 | 9.39 W × 9.65 L (reference dims) | 24 | LCSC datasheet https://datasheet.lcsc.com/datasheet/pdf/3a97d7e3ef6348a593e11a830413fcb3.pdf?productCode=C134113 (JAE drawing SJ116123, single sheet, top-left and mounting-condition views). LCSC **C134113**, stock 1307 |

### 1. OBD-C ports: full 24-pin receptacles (all SS contacts populated)

All parts in this table are female, horizontal, and have 24 contacts. The pin tables on the drawings list A2/A3/A10/A11/B2/B3/B10/B11 as SSTX/SSRX pairs.

| Rank | MPN (maker) | LCSC # / JLC stock | Mount | Total shell height | Above board / below board | Footprint (body W × L; W over legs) | Cutout & board | SMT-only? | Datasheet & where |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **TYPE-C-G06-1BBWC0.8** (XYECONN), "USB TYPE C/F 24PIN SMT MID-MOUNT 0.8" | **C22385227** / 454, Extended | Mid-mount (sink) 0.8 | **3.16** (dimensioned) | **2.36** derived (3.16 − 0.80) / **0.80** (dimensioned) | 8.94 × 7.90; 11.04 over legs | Cutout 9.24 wide (layout p.2). The layout note reads "THICKNESS 0.60MM" (quoted as printed; it does not name the board), and the part title says mid-mount 0.8. | Signal tails are two staggered top-side SMT rows (A1–A12, B1–B12 at 0.5 pitch). The 4 shell legs go into slots (2 × 1.60 × 0.60 and 2 × 2.00 × 0.60 per layout). | https://datasheet.lcsc.com/datasheet/pdf/d241aa9965671be6dde58e548092293e.pdf?productCode=C22385227 p.1 (front view 3.16, side view 0.80, 7.90, 8.94, 11.04); p.2 (PCB layout, 9.24, after-mating view) |
| 1= | **TYPE-C-31-M-27** (Korean Hroparts), "Laminated board" | **C707318** / 419, Extended | Mid-mount (sink) | **3.16** (dimensioned) | **2.40** (dimensioned, shell top to tail plane) / 0.76 derived (3.16 − 2.40) | 9.00 W (top view) × 7.90 L; 11.14 over legs; 8.64 shell | Board thickness not stated on drawing. Layout: tails on 2 rows; 4 oval shell slots (1.60 × 0.60 / 1.20 × 0.60). | Same as above | https://datasheet.lcsc.com/datasheet/pdf/3f1b097e0dae0e07bc9087d3b0be2cc0.pdf?productCode=C707318 single sheet (side view "3.16", "2.40"; top view 9.00 × 7.90; front view 11.14) |
| 2 | **HX-TYPE-C 24P-0.8** (hanxia) | **C49261589** / 1063, Extended | Mid-mount sink 0.8 | Not dimensioned directly. *Derived* 3.16 = opening 2.56 ±0.04 + 2 × 0.30 shell (material note "外壳 SUS304 T=0.30mm"). | 2.36 derived / **0.80 ±0.10** (side view) | 8.94 ±0.15 × 7.90 ±0.15; 11.00 ±0.15 over legs | Layout: cutout 9.40 wide. The dimension text gives 5.85/4.89/7.25 for the cutout-to-slot geometry. | Top-side 2-row SMT tails + 4 shell slots | https://datasheet.lcsc.com/datasheet/pdf/915ad3acbe51bcbaa043efa0cc4e5f4a.pdf?productCode=C49261589 single sheet |
| 2 | **HX-TYPE-C 24P-1.6** (hanxia) | **C49261591** / 3582, Extended | Mid-mount sink 1.6 | Derived 3.16 (same basis: 2.56 opening + 0.30 shell × 2) | 1.56 derived / **1.60 ±0.1** (side view) | 8.84 ±0.1 (top view) × 7.90 ±0.2; 11.00 ±0.15 over legs; shell 8.60 ±0.15 | Cutout **9.40** wide, 5.94 deep to the slot line (layout). Intended for 1.6 mm board. | Top-side 2-row SMT tails + 4 shell slots | https://datasheet.lcsc.com/datasheet/pdf/bbd6b3e583466963fdb241c1c0cc12e4.pdf?productCode=C49261591 single sheet (side view 1.60 ±0.1; layout 9.40, 5.94, 11.00) |
| 2 | **TYPE-C 24P QT CB1.6 / CB1.0 / CB0.8** (SHOU HAN). Same drawing family as hanxia. | **C49287223** (CB1.6) / 136; **C49287222** (CB1.0) / 1985; **C49287221** (CB0.8) / 452. All Extended. | Mid-mount sink 1.6 / 1.0 / 0.8 | Derived 3.16 (opening 2.56 ±0.04; shell thickness not given on this sheet) | CB1.6: 1.60 ±0.1 below. CB1.0 and CB0.8: sink per part name, I did not read their drawings. | 8.84 ±0.1 × 7.90 ±0.2; 11.00 ±0.15 | Cutout 9.40 (CB1.6 layout) | As above | CB1.6: https://datasheet.lcsc.com/datasheet/pdf/371417add220b0aa6deadd0200df555e.pdf?productCode=C49287223 p.1. CB1.0: https://datasheet.lcsc.com/datasheet/pdf/832edbc89af718986f4bd077ef0e2f7f.pdf?productCode=C49287222 |
| – | HX-TYPE-C 24P-1.0 (hanxia), "Laminated board" | C49261590 / 826 | Mid-mount 1.0 | not read | not read | 8.84 / 11.00 per text extract | – | – | https://datasheet.lcsc.com/datasheet/pdf/4adace2f962620202fd72a0812e6280c.pdf?productCode=C49261590 |
| ✗ | USB31-TYPE-C-FSABC (XKB), waterproof IPX7 mid-mount | C2880583 / 2352 | Mid-mount, "0.30 CENTER HEIGHT" | **4.02** (front view) | 1.40 "bottom shell of shell" | 9.20 × 9.65; 12.15 over legs | Layout "both sides are sample", Thickness 0.60 mm | – | https://datasheet.lcsc.com/datasheet/pdf/65e8c1c8ad8bac38d7659b2ae38473be.pdf?productCode=C2880583. Taller (4.02) than the plain parts; reject. |

**Shortest vertical 24-pin.** Vertical parts are the narrowest on the board, but much taller.

| MPN | LCSC / stock | Height | Footprint | Pins | Datasheet |
|---|---|---|---|---|---|
| **TYPE-C-G08B-2TDWH6.4** (XYECONN), "USB C/F 24PIN VERTICAL W/O EMI SPRING, HIGHT 6.4" | **C20883029** / 1049 | **6.40** above board | Body **8.94 × 3.16**; 4.30 over the tails; 7.50 between holes | 24 (2 staggered SMT rows + 2 pegs) | https://datasheet.lcsc.com/datasheet/pdf/04e057442764c17f9d3e89f0a58da1f0.pdf?productCode=C20883029 p.1 |
| TYPE-C-31-M-06A (Hroparts) | C3020878 / 86 | 6.4 mm (JLC description only; not read on drawing) | – | 24 | https://datasheet.lcsc.com/datasheet/pdf/b70d16ea478e9d4caab7c6495e2cd64b.pdf?productCode=C3020878 |
| JS24L-TYPE-C007-H650 (JUSHUO) | C49118466 / 1581 | 6.5 mm (JLC description; not read on drawing) | – | 24 | https://datasheet.lcsc.com/datasheet/pdf/03ef87ae273cf6029499743a02c0b0f6.pdf?productCode=C49118466 |

**Narrowest.**
- Every Type-C receptacle shell is about 8.6–9.0 mm wide (8.34 opening plus shell).
  - Mid-mount parts need a cutout about 9.24–9.40 mm wide and about 11.0–11.14 mm across the legs.
  - Vertical parts have the smallest board footprint (8.94 × 3.16 body) but stand 6.4 mm or more.
- With a bounding-box volume score, the horizontal mid-mount is far better. For one port:
  - mid-mount: about 11 × 7.9 board area, 3.16 tall;
  - vertical: 8.94 × 4.3 board area, but it forces the entire box to ≥ 6.4 mm.

**Dual-deck (stacked) Type-C.**
- They exist, but every one I found is through-hole and very tall.
- None of them is a full 24-contact part.
- Example: **Cvilux CU3616SASCLA004-LF**, "CU36 Double Deck Type C/F **16pin** L=17.70 H=15.40". It is 13.60 W, through-hole (title block and front view): https://mm.digikey.com/Volume0/opasdata/d220001/medias/docus/5858/CU36S04S1%28CU3616SASCLA004-LF%29.pdf p.1. It is USB 2.0 16-pin, so it is unusable for the CAN-on-SS pins.
- DigiKey also lists Amphenol MUSBRM5C130 and Assmann A-USBC-20F0-DA-GTP01 as stacked parts. Both are USB 2.0 / 16+8 dummy; I did not open their drawings.
- The JLC/LCSC catalogue search found no stacked Type-C in stock.
- Conclusion: two mid-mount singles side by side at 3.16 mm beat any stacked part (≥ 2 × 3.16, in practice 15.4 mm).

### 2. Host port: a 16-pin USB 2.0 receptacle is acceptable

| Rank | MPN (maker) | LCSC # / stock | Mount | Total height | Above / below board | Footprint | Cutout & board | Pins | Datasheet & where |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **TYPE-C 16P CB1.6 073** (SHOU HAN) | **C2906290** / **58,328**, Extended | Mid-mount sink 1.6 | **3.16** (dimensioned) | 1.56 derived / **1.60** (dimensioned) | 8.94 × **6.5**; 11.24 over legs | Cutout 9.24 wide. Layout note "THICKNESS 0.80MM", quoted as printed. Slots 4 × 0.60 wide. | 16 (single top-side SMT row, 0.5 pitch) + 4 shell slots | https://datasheet.lcsc.com/datasheet/pdf/982c12d8e453a2cd988955d221a11a82.pdf?productCode=C2906290 (drawing sheet: front 3.16 / 1.60, top 6.5 × 8.94, front 11.24, layout 9.24) |
| 1 | **GCT USB4500-03-0-A**, "Mid Mount, 0.8mm Offset" | **C5354966** / 28, Extended | Mid-mount 0.8 | **3.16** | 2.36 derived / **0.80** | 8.94 × 6.5; 11.24 over legs | Cutout 9.24; "Recommended PCB Layout Thickness=0.80mm" | 16 (+ optional shell spring) | https://gct.co/files/drawings/usb4500.pdf p.1 (side view 3.16 / 0.80; top 6.5 × 8.94; layout) |
| 1 | **GCT USB4510-03-1-A**, "Mid Mount, 1.6mm Offset" | **C5815192** / 131, Extended | Mid-mount 1.6 | **3.16** | 1.56 derived / **1.60** | 8.94 × 6.5; 11.24 over legs | Cutout 9.24 (layout note still reads "Thickness=0.80mm", as printed) | 16 | https://gct.co/files/drawings/usb4510.pdf p.1 |
| 2 | TYPE-C 16P CB0.8 073 (SHOU HAN) | **C2906289** / 11,392 | Mid-mount sink 0.8 | not read (drawing is past the cover pages) | – | 6.5 L per JLC description | – | 16 | https://datasheet.lcsc.com/datasheet/pdf/a7b245053bc71f176d30e4c6f44292bc.pdf?productCode=C2906289 (6-page approval spec) |
| – | GCT USB4520 (2.1 mm offset) | not checked | Mid-mount 2.1 | – | – | – | – | 16 | https://gct.co/files/drawings/usb4520.pdf |

### Notes for a 1-layer JLC board
- Single-sided tails:
  - All the recommended mid-mount parts have all signal tails as **top-side SMT pads**, so they are fine with copper on one side only.
  - The 24-pin parts have two staggered rows at 0.5 mm pitch with 0.25–0.30 mm pads. That needs fine copper, so check it against the JLC 1-layer trace/space rule.
  - Routing out of the inner B row with no vias is the hard part: the B row sits behind the A row.
- Shell legs:
  - The shell legs go through slots. On a 1-layer board those slots are unplated.
  - The legs can only be soldered to a top-side ring/pad around the slot, by paste wetting up the leg, or left mechanical.
  - I did not find a JLC statement either way.
- Plug engagement:
  - The plug engagement depth (4.70 on M-27 and 4.60 on GCT) is inside the 6.5–7.9 mm body length.
  - The plug overmould sits outside the board edge, as with any edge receptacle.

## 3. 12 V DC barrel jack (original: CUI / Same Sky PJ-002AH-SMT-TR)

### 3.1 What the original mates

Source: Same Sky PJ-002AH-SMT-TR datasheet, https://www.sameskydevices.com/product/resource/pj-002ah-smt-tr.pdf. Date 09/12/2024, p.2 "Mechanical Drawing". Unless noted, every figure below is from that page.

| Item | Value | Where |
|---|---|---|
| Center pin | Ø2.0 (+0/−0.05) mm | p.2 front view; also p.2 table "Center Pin Ø2.0 mm" |
| Jack opening | Ø6.50 mm | p.2 front view |
| Mating plug sleeve OD | Ø5.5 mm | p.2 "MATING PLUG" sketch |
| Jack insertion depth | 9.5 mm | p.2 under "MATING PLUG" |
| Body height | 11 ±0.2 mm | p.2 front view |
| Body width | 9 ±0.2 mm | p.2 front view |
| Body length | 14 ±0.2 mm (14.9 ±0.3 including rear terminal) | p.2 top and side views |
| Footprint pad span | 17 mm across the SMT pads | p.2 "Recommended PCB Layout" |
| PCB thickness assumed | 1.6 mm | p.2 notes |
| Ratings | 24 Vdc, 5 A | p.1 specifications |

The jack therefore mates the standard **5.5 mm OD plug with a 2.1 mm ID** (a "5.5/2.1" plug, sold to fit a Ø2.0 mm pin). A substitute must have a Ø2.0 mm center pin and accept a Ø5.5 mm sleeve. Jacks with a 2.5 mm pin will not accept the plug, and jacks with a 1.65 mm or 1.3 mm pin make poor contact with it.

**Assembly height of the original:** 11 mm above the board top, plus the board.

### 3.2 Candidates (Ø2.0 mm pin, mates a Ø5.5 mm plug), lowest first

"Above board" is measured from the board top surface (the SMT/seating plane). "Below" is how far the body or pin tails go below that plane. A mid-mount jack sits partly inside the board thickness, so for it the assembly height is about max(above + below, above + board thickness).

| MPN | Mount | Above board | Below board top | Body L×W | Pins / terminations | Cutout / PCB thickness | 1-layer OK? | LCSC # / JLC | Datasheet & location |
|---|---|---|---|---|---|---|---|---|---|
| **G-Switch DC-S007A-D020-15A** | **Mid-mount ("DIP Sink 0.93")**, right-angle | **5.92** (derived: 6.85 total − 0.93 sink) | Body 0.93; center-pin tail tip 1.51 (derived: 7.43 to the tail tip − 5.92) | 12.50 × 9.40 (12.05 across the shell tabs) | 1 center-pin tail (Ø1.50 hole) + 4 shell tabs (T = 0.25) in 0.6 × 1.7 slots. All are through-board, none SMT | Cutout 9.80 wide × 11.80 deep from the board edge (layout 14.50 × 12.05 overall). **PCB thickness not stated** | Yes. Tabs and tail solder on the copper side, through unplated holes/slots | **C20611871**, Extended, stock 243 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401191747_G-Switch-DC-S007A-D020-15A_C20611871.pdf, single sheet. Front view: 6.85, 0.93, 9.40, 2.50 (CH). Side view: 12.50, 7.43 ±0.30. "Recommended PCB layout": 9.80, 11.80, 14.50, 12.05. Title block: "DIP Sink0.93". Mating plug Ø5.5 × Ø2.1 is drawn and labelled. Rated 15 A / 30 V |
| **XUNPU DC-503-ARF200-CS014** | Mid-mount (sink), right-angle. Same outline as G-Switch | 5.42 (derived: 6.85 − 1.43) | Body 1.43; tail tip 2.00 (derived: 7.42 − 5.42) | 12.50 × 9.40 | Same as above: center-pin tail Ø1.50 + 4 tabs | Cutout 9.80 × 11.80 (layout 14.50 / 12.05). PCB thickness not stated | Yes (through-board tabs) | **C30607512**, Extended, stock 42 | https://www.lcsc.com/datasheet/lcsc_datasheet_2407221709_XUNPU-DC-503-ARF200-CS014_C30607512.pdf, single sheet. Front view: 6.85, 1.43, 2.00 (CH). Side view: 7.42 ±0.30. Layout: 9.80 / 11.80. Mating plug Ø5.50 / Ø2.10 drawn. Rated 30 V 15 A |
| HOOYA (Haoyu) DC-044G-2.0-PU-G | Top-mount, right-angle, **SMT** | 7.00 ±0.1 | No pins below (2 plastic pegs into Ø2.30 holes; peg length not dimensioned) | 12.50 ±0.3 × 9.50 ±0.1 (11.50 ±0.1 body depth) | 3 SMT pads + 2 locating pegs | No cutout. PCB thickness not stated | **Yes, SMT-only** | **C19190846**, Extended, stock 495 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2311101855_HOOYA-DC-044G-2-0-PU-G_C19190846.pdf, single sheet. Front view: 9.50, Ø6.0 opening, Ø2.00 pin, 3.50 to pin center. Side views: 7.00 ±0.1, 11.50 ±0.1, 12.50 ±0.3. "P.C.B layout top view". **The mating-plug OD is not given on the drawing.** A Ø6.0 opening with a Ø2.0 pin is the DC-044 "5.5/2.1" family, but confirm this before committing |
| XKB DC-0440-2.5A-2.0 | Top-mount, right-angle, THT | 7.00 | Pin tails 4.00 below the seat | 11.50 × 9.50 | 3 THT slots (2.0 × 0.8) + 2.0 × 2.0 peg | No cutout | Yes (THT pins soldered on the copper side) | **C381118**, Extended, stock 1610 | https://www.lcsc.com/datasheet/lcsc_datasheet_2110151630_XKB-Connection-DC-0440-2-5A-2-0_C381118.pdf, single sheet. Front view: 7.00, 9.50, 3.50, Ø6.00. Side view: 11.50, 4.00. Mating plug drawn with "5.50" and "2.60" (the 2.60 callout is ambiguous). Insertion depth 8.2 mm |
| Same Sky PJ-083AH | Top-mount, right-angle, THT | 7.20 ±0.3 | Pin tails 2.50 ±0.3 below the seat | 11.5 ±0.3 × 10.00 ±0.3 | 5 THT tails | No cutout | Yes (THT) | C3094071, Extended, **stock 0** | https://www.sameskydevices.com/product/resource/pj-083ah.pdf p.2. Front view: 7.20, 3.50 to center, Ø5.90 opening, 10.00, 2.50 (5×). Mating plug p.2: center pin Ø2.0, insertion depth 8.2 mm |
| Same Sky PJ-051AH | Top-mount, right-angle, THT | 7.3 | Pin tails 2.5 (5×) | 11.5 × 10.1 | 5 THT | No cutout | Yes | C22359773 / C3095050, **stock 0** (PJ-051A, the 2.5 A version: C3095350, stock 24) | https://www.sameskydevices.com/product/resource/pj-051ah.pdf p.2. Front view: 7.3, 3.50, Ø6.00, 10.1, 2.5 (5X). Mating plug Ø2.0 pin, depth 8.2 mm |
| Same Sky PJ-047AH | Top-mount, THT (narrow) | **9.75 ±0.3**. The Same Sky catalog's "7 mm" is wrong | 2.65 ±0.3 tails | 11.5 × 7.6 | 3 THT | No cutout | Yes | C3095055, stock 40 | https://www.sameskydevices.com/product/resource/pj-047ah.pdf p.2. Front view: 9.75, 6 to center, 7.6. Plug Ø5.5 drawn |
| Same Sky PJ-107AH-MSMT-TR | Mid-mount, right-angle | 5.45 ±0.3 | 3.65 (ref) | 11.7 ±0.3 × 10.00 ±0.3 (13.1 across the tabs) | Oval plated slots (THT-style tabs) | Cutout ≈10.3 wide (6 + 4.3 in the layout), open to the board edge. **PCB thickness 1.6 mm** | Yes (tabs) | Not at JLC (search returned 0) | https://www.sameskydevices.com/product/resource/pj-107ah-msmt-tr.pdf p.2. Front view: 5.45, 10.00, Ø6. Side view: 9.1 ±0.3 total, 11.7. Rear view: (3.65). Notes: PCB 1.6 mm. Mating plug Ø5.5, depth 8.1 mm |
| XUNPU DC-028S-A200 (same outline as Kycon KLDHCX-MM-0202-A, Hroparts DC-028S-20A) | Mid-mount SMT | Not split in the datasheet | Not split | 11.60 × 10.00 | SMT tabs + center-pin tail | Cutout per layout (11.60 / 9.90 / 13.00). Thickness not stated | Yes | C720579, stock 144 | https://www.lcsc.com/datasheet/C720579.pdf p.1. Total H 9.10, W 11.60, D 10.00, Ø6.00 opening, Ø2.0 pin |

Rejected after checking the datasheet: XKB DC-005E-2.5A-2.0 (C500096). LCSC lists a "6.4 mm body height", but the drawing shows an 11.0 mm body with the pin center at 6.45 mm (single sheet, front view). It is no lower than the original.

### 3.3 Recommendation (barrel jack)

* **Lowest: G-Switch DC-S007A-D020-15A (C20611871).** It is 5.92 mm above the board top and goes 0.93 mm (body) / 1.51 mm (pin tail) below it. On a 1.6 mm board nothing protrudes below the board, so the assembly is about **7.5 mm** thick (5.92 + 1.6), against 11 + 1.6 = 12.6 mm for the original. The datasheet explicitly shows the Ø5.5 × Ø2.1 mating plug. The XUNPU DC-503 is a second source with the same outline (5.42 above, 1.43 below the board top).
* **Lowest top-mount, SMT-only, no cutout:** HOOYA DC-044G-2.0-PU-G (C19190846), 7.0 mm above the board. Confirm the plug OD with the vendor first.
* Physics floor: a Ø5.5 mm plug inside a jack with walls cannot go much under about 6.8 mm total. Of that, about 5.4–5.9 mm sits above the board when the jack sinks into a 1.6 mm board.

---

## 4. microSD socket (original: Molex 47219-2001 / 0472192001)

### 4.1 Original

* Type: hinge (flip-top), 8 contacts, 1.10 mm pitch, no card detect. Source: Molex product page PDF mirrored by Farnell, https://www.farnell.com/datasheets/2157450.pdf p.1, "Description" / "Card Detection Switch: No" / "Circuits 8".
* **Height and footprint: the Molex sales drawing (SD-47219-001, https://www.molex.com/pdm_docs/sd/472192001_sd.pdf) could not be retrieved.** molex.com timed out from this machine. JLC/LCSC list C164170 with "Height Above Board 1.9 mm, Length 14.5 mm, Width 13.6 mm" (secondary source, parametric data only), and RS lists 13.6 × 14.5 × 1.9 mm. Treat **1.9 mm** as the unverified original height. LCSC C164170, stock 8983.

### 4.2 Candidates (all standard top-mount SMT, all 8 contacts plus ground/detect tabs). None needs a cutout.

| MPN | Height above board | Body L × W (mm) | Ejection | Card detect | LCSC # / JLC stock | Datasheet & location |
|---|---|---|---|---|---|---|
| **XUNPU TF-128-ARP9** | **1.15** | 8.06 × 11.52. The card sticks out 7.34 (ref) past the body | Push-pull (friction) | Yes (P6/CD, normally open) | **C5174689**, Extended, stock 2095 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2209151800_XUNPU-TF-128-ARP9_C5174689.pdf, single sheet. Side view 1.15; top view 11.52 / 8.06 / 7.34 (ref); title "Micro SD H1.15 CARD CONN" |
| GCT MEM2085-00-115-00-A | 1.15 | 8.06 × 11.52 (same outline as TF-128) | Push-pull | Yes (normally open) | C7301405, **stock 0** | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401301656_Global-Connector-Technology-MEM2085-00-115-00-A_C7301405.pdf p.1: 1.15, 11.52, 8.06, title "Push-Pull, SMT, 1.15mm Profile" |
| Hirose DM3NW-SF-PEJ(800) | 1.15 | 15.30 × 13.40 | **Push-push** | Yes (normally closed) | C6832162, **stock 0** | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2312271159_HRS-Hirose-DM3NW-SF-PEJ-800_C6832162.pdf (Hirose flyer MKT-E-16068F_1), "Dimensions" figure: 1.15 mm, 13.40 mm, 15.30 mm |
| **JAE ST11S008V4HR2000** | **1.25** | 15.3 deep (17 with the card) × 13.4 wide | **Push-push** | Yes (D-SW, normally closed); 8 + 2 contacts | **C3177044**, Extended, stock 6055 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401291219_JAE-Electronics-ST11S008V4HR2000_C3177044.pdf p.1, Features: "1.25mm height, 13.4mm width, and 15.3mm depth (depth including card: 17mm)" |
| Molex 503398-1892 (5033981892) | 1.28 (LCSC parametric only) | 14.05 × 13.1 (LCSC parametric) | Push-push | Not stated | C428492, stock 9764 | Molex drawing not retrievable (molex.com timeout). **Dimensions unverified** |
| Molex 104031-0811 | 1.42 ±0.15 | 11.40 ±0.15 × 11.95 ±0.15 | Push-pull | Yes | C585350, stock 1324 | https://www.lcsc.com/datasheet/lcsc_datasheet_2407081302_MOLEX-1040310811_C585350.pdf, sheet 1 of 2 (SD-104031-001): side view 1.42, top view 11.95 / 11.40, title "PUSH-PULL 1.42H" |
| SOFNG TF-009 | **1.45 ±0.10 per the drawing** (LCSC lists 1.15, which is wrong) | 14.95 × 14.14 | Push-push | Yes | C428466, stock 499 | https://www.lcsc.com/datasheet/lcsc_datasheet_1912111437_SOFNG-TF-009_C428466.pdf p.1: header "15.00L×14.15W×1.45H", side view 1.45 ±0.10 |
| Amphenol 114-00841-68 | 1.55 ±0.13 | 11.45 × 11.95 | Push-pull ("None Push") | Yes | C3195424, stock 15950 | https://www.lcsc.com/datasheet/C3195424.pdf, sheet 1/2: side view 1.55 ±0.13, top view 11.95 / 11.45 |
| Hirose DM3D-SF | 1.55 (LCSC parametric) | 11.45 × 11.95 (LCSC parametric) | Push-pull | No | C719027, stock 115 | Hirose PDF downloaded (https://www.lcsc.com/datasheet/C719027.pdf, 12 pp.) but these figures were not checked against it |
| Hirose DM3AT-SF-PEJM5 | 1.68 (LCSC parametric) | 15.95 × 13.85 | Push-push | No | C114218, stock 15249 | Not verified against the datasheet |
| Hroparts TF-01A | 1.85 (LCSC parametric) | 14.5 × 14.8 | Push-push | Varies | C91145, stock 166540 | Not verified. No lower than the original |

**Mid-mount / reverse-mount microSD.** No microSD socket was found that sits in a through-board cutout to reduce height. The "reverse-mount" types mount on the bottom side and pass the card through a cutout, and they are taller than the best top-mount parts:

* Molex 502774-0891: 1.80 mm (DigiKey listing)
* Hirose DM3BT-DSF-PEJS: ≈1.83 mm (DigiKey)
* SMK CLE9108-7A09F: 1.32 mm (SMK press release, https://smkchina.smk.co.jp/news/press_release/2013/1010cs)
* JST SDHR-8BRS-363-ETB: 1.45 mm (distributor listing)

None of these were verified against a datasheet, and none beat a 1.15 mm top-mount.

**Not usable:** XUNPU MCS-303-xx (C694252 "1.0 mm", C391010 "0.86 mm"). The datasheet title reads "NANO SIM + NANO SIM + TF 三選二卡座" (https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2006291835_XUNPU-MCS-303-KT1_C694252.pdf p.1). These are phone tray-type SIM/TF combo sockets that need a tray and an eject pin, not a bare microSD socket.

### 4.3 Recommendation (microSD)

* **Lowest and stocked at JLC: XUNPU TF-128-ARP9 (C5174689), 1.15 mm**, 11.52 × 8.06 mm body, push-pull. This saves 0.75 mm against the original's (unverified) 1.9 mm and also has the smallest footprint. Caveat: in a push-pull socket the card sticks out 7.34 mm (ref) past the body. If an inserted card counts toward the bounding box, place the socket so the card overhangs the board edge or lies inside the outline.
* **Lowest push-push that JLC actually stocks: JAE ST11S008V4HR2000 (C3177044), 1.25 mm**, 15.3 × 13.4 mm.
* The Hirose DM3NW is also 1.15 mm and push-push, but JLC has 0 stock.
* All of these are SMT-only, so they suit a 1-layer board (copper on the top side only).

### Notes on sources and JLC status

* Stock figures come from the JLCPCB parts API (`selectSmtComponentList`), queried 2026-10-27. "Extended" means a JLC Extended-library part, which JLC assembles for an extended-part loading fee. THT/mid-mount jacks need JLC's through-hole assembly option.
* "Derived" figures are arithmetic on dimensions quoted from the drawing, not values printed on it.

## 5. CAN header J4 (2x4, 2.54 mm male) — low-profile substitutes

What must mate: a standard 2x4, 2.54 mm female housing or IDC. Every option below keeps 0.64 mm square posts at 2.54 x 2.54 mm pitch with a 6 mm mating post, so the female side does not change.

JLC stock and Basic/Extended status come from the JLCPCB parts-search API (`componentLibraryType`: `base` = Basic, `expand` = Extended), queried 2026-10-27.

| Option | MPN (mfr) | LCSC | Mount | Height above board | Below board | Footprint | Pins | SMT-only? | JLC | Datasheet / where |
|---|---|---|---|---|---|---|---|---|---|---|
| A. Right-angle THT (lowest found) | HX PZ2.54-2x4P WZ-P (Hanxia) | C42453962 | Right angle, pins parallel to board, pointing off the edge | Insulator 5.00 mm tall. Lower row 2.54±0.2 above board. Top of the upper pin bend is 6.90±0.2 above board. Mating posts are 6.00±0.2 long, horizontal. | Tails 3.00±0.2 measured from the seating plane, so they protrude (3.0 − board t) below the board. | Pins on a 2.54 mm grid, 10.16 x 2.54 mm hole pattern for 8P (table "No. 4: A=7.62, B=10.16"). Holes Ø1.02. | 8 | No. THT, but the tails can be soldered on a 1-layer board (pads on the bottom copper, body on the bare top). | Extended, 681 in stock | https://datasheet.lcsc.com/datasheet/pdf/390f742563af3afc61afeddd5c428348.pdf?productCode=C42453962 (single page, side view and table) |
| B. Vertical SMT | 2.54-2*4P vertical SMD (BOOMELE) | C192300 | Vertical SMT, gull-wing feet | 3.20±0.2 (feet and insulator) + 6.00±0.2 post = about 9.2 mm | 0 | Feet span 7.50±0.2. PCB pad layout 9.00±0.2 across, 2.00 x 1.0 pads, 2.54 pitch, 4 pads per side. | 8 | Yes | Extended, 8,188 in stock | https://datasheet.lcsc.com/datasheet/pdf/8275dc04b158fe5c6f9bb3a86491ae78.pdf?productCode=C192300 (single page, side view rotated) |
| C. Vertical THT (baseline) | HX PZ2.54-2x4P ZZ (Hanxia) | C32713277 | Vertical THT | Listing: 2.5 mm insulator + 6 mm post = about 8.5 mm. Datasheet not opened. | Listing: 3 mm tail | 2x4 grid at 2.54 mm | 8 | No | Extended, 60,373 in stock | Listing only (JLC parts API description). Not verified in a datasheet. |
| D. Straddle / edge-mount | none found in JLC/LCSC stock | — | — | — | — | — | — | — | — | Searches for "2x4P 2.54 header edge" returned only pluggable terminal blocks (KF2EDGE). |

Notes on J4:
- **Right-angle (A) is the lowest stocked option.** It stands at most 6.90 mm above the board, against about 9.2 mm (SMT vertical) or about 8.5 mm (THT vertical). The female plug then inserts horizontally from the board edge, so a plug that is not counted in the score does not add height. If the scored volume includes the mated plug: a standard 2x4 female housing is about 5.1 mm across its two rows, centred on rows at 2.54 and 5.08 mm, so it would span about 1.3–6.4 mm above the board. That stays within the header's 6.90 mm. This assumes the generic housing width; I did not verify it in a female-housing datasheet.
- **The vertical options add the plug on top.** A standard female 2x4 header is around 8.5 mm tall (generic figure, not verified in a datasheet here), so plug and header together come to roughly 17 mm or more.
- **Straddle mounting does not work on a 1-layer board.** A dual-row straddle header solders one row to each face, and a 1-layer board has copper on one side only. The 2.54 mm row pitch also implies a board of about 1.9 mm (2.54 − 0.64), not 1.6 mm. That gap figure is my arithmetic, not a datasheet value.
- **No dual-row right-angle SMT 2.54 mm male header was found in stock.** Searches turned up only 2.0, 1.27 and 1.5 mm pitch dual-row SMT right-angle parts, plus single-row XH-type parts with 0 stock.
- **Option A works on a 1-layer board.** JLC drills 1-layer holes 0.3–6.3 mm, all non-plated, so the Ø1.02 holes are fine. The tails solder to single-side pads with no barrel.

## 6. Push button SW1 — tactile switch alternatives

**Original part, confirmed from the datasheet.** Panasonic EVQ-Q2 ("Light Touch Switches/EVQP0/Q2, 6 mm Square Thin Type SMD", Jun 2008). Datasheet copy: https://datasheet.octopart.com/EVQQ2201W-Panasonic-datasheet-5314136.pdf
- **Body:** 6.5 x 6.0 mm, 1.8 mm tall excluding the push plate (p.1, Features).
- **Height H by variant:** 2.0±0.2, 2.5±0.2 or 3.1 +0.3/−0.1 mm (p.3, "Dimensions No. 2 EVQQ2").
- **Land pattern:** 3.2 / 3.6 / 3.2 mm, so 10.0 mm overall x 4.0 mm (p.3, "PWB land pattern for reference").
- **Terminals:** J-bent, SMT, 4 terminals.
- **Travel:** 0.25 mm.

All candidates below are top-actuated SMT switches with nothing below the board, so all are fine on a 1-layer board.

| MPN (mfr) | LCSC | Height above board | Body L x W | Land / footprint | Pins | JLC | Datasheet / where |
|---|---|---|---|---|---|---|---|
| **TS3735A 250gf 030** (Shou Han), dome-and-film type | C2906282 | **0.35 +0.10/−0.05 mm** | 3.70±0.05 x 3.70±0.05 | 4 pads 0.65 x 0.60, pad centres 3.05 x 2.70 | 4 | Extended, 15,152 in stock | https://datasheet.lcsc.com/datasheet/pdf/ccb4e9c631590958e20642766e149b66.pdf?productCode=C2906282 (p.1 drawing, title "轻触开关3.7*3.7*0.35"). Same family: C5143616 (Hanxia), C2888443 (BZCN, listed 0.35 mm, datasheet text unreadable). |
| **KXT321LHS** (C&K KXT3) | C221821 | **0.6±0.1 mm** total | 3.0 x 2.0 (3.5 over tabs) | 2 pads, 3.8 outer / 2.7 inner x 1.5 | 2 | Extended, about 6,000 in stock (LCSC) | https://datasheet.lcsc.com/datasheet/pdf/41e1d09331043e2917dc8c0403e63ca2.pdf?productCode=C221821 (p.1 "0.6 mm thickness, footprint 3.0 x 2.0"; p.2 (B-6) drawing and PCB layout) |
| **SKSWCEE010** (Alps SKSW) | C202371 | **0.6±0.1 mm** | 3±0.05 x 2±0.1 (3.5±0.1 over terminals) | 2 pads, 3.8 / 2.7 x 1.5 | 2 | Extended, 13,640 in stock (LCSC) | https://datasheet.lcsc.com/datasheet/pdf/52aa5d5e8b9d16533a1d5b940b939a4e.pdf?productCode=C202371 (p.2 "Dimensions", "Land Dimensions") |
| GT-TC024A-H0060-L1 (G-Switch), 3x2 type | C778126 | 0.6 mm (listing only) | 3 x 2 (listing) | not checked | — | Extended, 17,723 in stock | Listing only. Datasheet not opened. |
| TPS850 J14M V SMTR LFS (ROCPU) | C2759974 | 1.4 mm | 3.00±0.1 x 2.75±0.1 | land about 3.65 x 2.85 | 4 | Extended, 3,840 in stock | https://datasheet.lcsc.com/datasheet/pdf/3caa06c4c77243e2b2986a3864571398.pdf?productCode=C2759974 (p.1 table "Height 1.4"; p.2 drawing) |
| TS-1187A-B-A-B (XKB) | C318884 | 1.5±0.05 mm (code A) | 5.1±0.05 x 5.1±0.05 (6.5±0.25 over terminals) | pads at 7.0 / 5.0 x 4.5 / 3.0 | 4 | **Basic**, 1,165,999 in stock | https://datasheet.lcsc.com/datasheet/pdf/56c8799ae5193945a16a1ffbe378246a.pdf?productCode=C318884 (single page, height table "Code A = 1.5") |
| KMR221GLFS (C&K KMR2) | C72443 | 1.9 mm | 4.2 x 2.8 | not extracted | 4 | Extended, 3,764 in stock | https://datasheet.lcsc.com/datasheet/pdf/b3b49c9e2af6b33c7691f4b109b7083c.pdf?productCode=C72443 (p.1 "Total height 1.9 mm"; p.2 drawing) |
| SKRPACE010 (Alps SKRP) | C139797 | 2.5 mm (1.8 to the body top) | 4.2 x 3.2 (4.6 over terminals) | 5.2 / 3.1 x 2.8 / 1.5 | 4 | LCSC stock 70,810. JLC status not checked. | https://datasheet.lcsc.com/datasheet/pdf/0d81285c392e478b95c7e9b3b68f85ec.pdf?productCode=C139797 (p.2 dimensions and land) |

Notes on SW1:
- **Thinnest overall:** the TS3735A-type, a 0.35 mm metal dome on a film, still pushable.
- **Smallest area:** KXT3 or SKSW, 3.0 x 2.0 x 0.6 mm. Both have 2 terminals; EVQ-Q2 has 4 internally paired terminals, so a 2-pad footprint is electrically the same SPST.
- **Basic part:** TS-1187A (C318884) is the only Basic one, which avoids the extended-part fee, but it stands 1.5 mm.

## JLCPCB rules: 1-layer boards and assembly

| Item | Rule | Source |
|---|---|---|
| FR-4 thickness list | 0.4 / 0.6 / 0.8 / 1.0 / 1.2 / 1.6 / 2.0 mm | https://jlcpcb.com/capabilities/pcb-capabilities ("Thickness for FR4 ...") |
| 1-layer thickness exclusions | 0.4 mm: "not available for 1-layer PCBs". 0.6 mm: "not available for 1-layer, 4-layer, or 6-layer PCBs". **So the thinnest 1-layer FR-4 is 0.8 mm.** | https://jlcpcb.com/resources/pcb-thickness |
| 2-layer thickness | 0.4 mm is allowed (excluded only for 1-layer). The only 0.6 mm caveat: "2-layer PCBs with 0.6mm thickness do not support HASL (with lead)". | https://jlcpcb.com/resources/pcb-thickness |
| Min trace / space (1 oz) | 1- and 2-layer: 0.10 / 0.10 mm (4/4 mil) | pcb-capabilities |
| Plating on 1-layer | Holes are non-plated only. "1-layer (NPTH only): 0.3 mm hole size / 0.5 mm via diameter". | pcb-capabilities (Drilling / Via) |
| Drill range, 1-layer | 0.3 – 6.3 mm | pcb-capabilities |
| Min NPTH (general) | 0.50 mm, drawn on the mechanical or keep-out layer | pcb-capabilities |
| Slots / cutouts | Non-plated slot min width 1.0 mm, drawn on the mechanical layer (GM1/GKO). Plated slot min 0.5 mm (2-layer only). Slot length ≥ 2x width. Square-cornered slots not supported. | pcb-capabilities |
| Outline | Copper ≥ 0.2 mm from a routed edge. Routed edge tolerance ±0.2 mm. Min board 3 x 3 mm. Max 1-layer FR-4 606 x 510 mm (≥ 0.8 mm thick). | pcb-capabilities |
| OSP finish | Not available on single-sided FR-4 | pcb-capabilities |
| Economic PCBA | Layers "2,4,6" only, thickness 0.8 – 1.6 mm, single-sided placement (SMT and THT), board 10x10 to 470x500 mm, min package 0402 | https://jlcpcb.com/capabilities/pcb-assembly-capabilities |
| Standard PCBA | Layers "1 - 32", thickness "No limit", single- and double-sided placement, board 70x70 to 460x500 mm (smaller boards need panelising up to that size), min package 01005 | pcb-assembly-capabilities |
| THT assembly | Offered on Economic and Standard (wave soldering for THT) | pcb-assembly-capabilities (FAQ) |

What the rules mean for a 1-layer build:
- **Standard PCBA only.** A 1-layer board can only be assembled under Standard PCBA; Economic PCBA does not list 1-layer.
- **0.8 mm minimum.** Because fab will not make 0.4 or 0.6 mm 1-layer boards, the thinnest assembled 1-layer board is 0.8 mm. (Standard PCBA itself sets no thickness limit.)
- **2-layer assembly thickness.** With Standard PCBA a 2-layer board can go down to 0.4 mm. With Economic PCBA the minimum is 0.8 mm.
- **Unconfirmed.** I found no help article that confirms 1-layer assembly by name beyond the "1 - 32 layers" row. Check it in the online quote before committing.
