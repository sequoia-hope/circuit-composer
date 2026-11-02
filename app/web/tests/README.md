# Circuit Composer End-to-End Tests

Playwright end-to-end test suite for the Circuit Composer web app (`app/web`).

## Prerequisites & Server Resolution

- Uses the system Google Chrome (`/usr/bin/google-chrome`) — no Playwright browser downloads.
- Tests run against the already-built `app/dist/` bundle served by the project's registered server.
- `playwright.config.js` resolves `baseURL` at runtime from `process.env.PORT` (`http://localhost:$PORT/app/dist/`) or the output of `proj url circuit-composer` (with `app/dist/` appended), throwing an explicit error if neither succeeds. It never hardcodes a port or starts an ad-hoc server.
- Two Playwright projects run by default:
  - `desktop` — `1440×900`
  - `mobile` — Pixel 7 (`412×915`, `isMobile: true`, `hasTouch: true`)
- Failure screenshots (`only-on-failure`) and failure traces (`retain-on-failure`) are written to `app/web/test-results/`.

## Running

Always book CPU cores through `~/.claude/bin/reserve` when running the suite. From `app/web`:

```bash
~/.claude/bin/reserve -c 4 -l "e2e" -- npm run test:e2e -- --workers=4
```

Or equivalently via `npx`:

```bash
~/.claude/bin/reserve -c 4 -l "composer e2e" -- npx playwright test --workers=4
```

`playwright.config.js` also picks up `process.env.RESERVED_CORES` automatically when `--workers` is not passed on the command line. Note: exit code `75` from `reserve` means cores could not be booked and the suite did not run.

To run a single project or spec file:

```bash
~/.claude/bin/reserve -c 4 -l "e2e" -- npm run test:e2e -- --workers=4 --project=desktop tests/workspaces.spec.js
```

## What Each Spec Covers

- **`shell-and-boards.spec.js`** — Verifies initial app shell load (header, workspace tabs, board selector, desktop mode chrome), clean startup across every example board (no page errors and no failed requests except `api/info`, the probe for Composer's server, a 404 on a static server by design), opening all four example boards (`bolt-earring`, `easy2350`, `flame-earring`, `rp2350-battery`) from the selector and confirming WebGL2 canvas rendering with distinct non-blank pixels, and switching between `Drawn` and `Solved` floorplans on boards with a loose composition.
- **`workspaces.spec.js`** — Verifies switching across all five workspaces (`Circuit`, `Board`, `Shape`, `Copper`, `Rules`) via header tabs and number keys `1`–`5` (including leaving `Shape`), URL `?view=` updates and restoration across reloads, cross-workspace block selection (`Circuit` → `Board`), adding and applying an outline in `Shape`, switching/collapsing `Dock` tabs (`Checks`, `AI jobs`, `Terminal`), and `?embed=1` compact bar mode.
- **`layers-and-inspector.spec.js`** — Verifies toggling all six floorplan layer checkboxes (`Top`, `Bottom`, `Ratsnest`, `Labels`, `Block colours`, `Locks` — directly on desktop and via the `View` menu on mobile) without errors and confirming visual canvas changes when `Locks` is toggled; opening the parts `Inspector` (directly on desktop, via `Parts` on mobile), searching for a part by reference (`U4`), inspecting its metadata/nets, rotating + undoing (`Ctrl+Z`), locking/unlocking, and using `Select locked` / `Select free` on `flame-earring`.
- **`placement.spec.js`** — Runs a short floorplan placement (`Place unlocked parts` with the smallest `50 k` moves setting) and verifies the `% packed` packing metric updates upon completion; also verifies terminating an active worker placement via `Stop` while preserving valid metrics.
- **`mobile.spec.js`** — Mobile-only checks (`412×915`): verifies zero horizontal viewport overflow (`document.documentElement.scrollWidth <= window.innerWidth`) across all workspaces and open mobile drawers/menus, verifies header wrapping (desktop-only items hidden, workspace `<nav>` wrapped onto its own row below the board selector) and tap reachability of all tabs, checks `>= 32px` touch target dimensions, and checks that the Circuit graph's block names stay at least 8 px tall.
- **`theme.spec.js`** — Emulates `light` and `dark` `colorScheme` settings, verifying CSS theme tokens, computed backgrounds, non-blank WebGL2 board rendering in both themes, and live canvas redrawing when `prefers-color-scheme` changes dynamically.
- **`helpers.js`** — Shared utilities for diagnostics (`console`, `pageerror`, `requestfailed`, HTTP `>= 400`), readiness waits, and compositor screenshot pixel analysis (`OffscreenCanvas`).
