# UI polish 2

Five small UI fixes. Each section lists objective, files, logic, decisions, risks and tests.

## 1 — Empty board uses the plain "+" add-lane button

- Objective: a board with no lanes showed a wide "+ New lane" button; it now shows the same square **+**
  button used when lanes exist.
- Files: `src/lib/board/BoardView.svelte`.
- Logic: removed the `{#if !lanes.length}` label; the button always renders only the `Plus` icon, with
  `aria-label={t('lanes.new')}` and the existing tooltip (`lanes.new` + `lane.new` keybinding). Same class,
  size and position (first slot of `.lanes`).
- Decision: the empty-state hint below ("A calm, empty board / Add a lane to get started") already explains
  what to do, so the text label was redundant. No other BoardView structure touched (scroll containers are
  being changed in parallel).
- Tests: visual check in `pnpm dev:web` (new empty board, light and dark); accessible name verified in the DOM.

## 2 — Softer "Hawaii sunset" tint

- Objective: less red, more transparent tinted surfaces; still pastel sunset; primary accent unchanged.
- Files: `src/styles/tokens.css`, `src/lib/theme/theme.svelte.ts`.
- Logic:
  - Light: `--bg`, `--bg-elev`, `--bg-sunken`, `--bg-card` moved from pink sand to apricot cream; translucent
    layers lowered (`--bg-glass` 0.72 → 0.60, `--bg-sidebar` 0.66 → 0.50, `--bg-lane` 0.62 → 0.55) and their
    hue neutralised; hover/active/line/shadow tints moved from plum-red `rgb(90 50 60)` to lavender-grey
    `rgb(70 55 80)` / `rgb(60 45 70)`.
  - Dark ("Hawaii dusk"): surfaces lean lavender-grey instead of red-plum; `--bg-glass` 0.72 → 0.62,
    `--bg-sidebar` 0.60 → 0.50; hover/line tints use `rgb(245 240 255)`.
  - `--sunset` gradient shifted from coral/hibiscus toward apricot/peach/soft lavender in both themes.
  - Runtime-derived accent tints (`theme.svelte.ts`): light `--primary-soft`/`--primary-softer` chroma
    0.35/0.20 → 0.28/0.16; dark alpha 0.16/0.08 → 0.14/0.07. `--primary` and `--primary-strong` unchanged.
  - `--ink-3` (light) `#85727c` → `#7a6873` so tertiary text meets WCAG AA (≥ 4.5:1) on bg, sunken and lane
    surfaces (it was 3.9–4.2:1 before).
- Contrast (computed): light ink 13.8–15.2, ink-2 7.9–8.9, ink-3 4.6–4.9; dark ink 13.3–15.2, ink-2 8.7–9.9,
  ink-3 4.6–5.3.
- Decisions: lane colours chosen by the user (`tintFromHex`) and the user's primary/secondary colour settings
  are left as they are; only the theme's own surfaces changed.
- Risks: native vibrancy (Liquid Glass / sidebar material) shows more through the lower-alpha layers. This
  is intended, but it was only checked in the web build (no glass there); check the macOS build.
- Tests: visual check, light and dark, start page, board and empty board, in `pnpm dev:web`.

## 3 — Window can be dragged from the top bar

- Objective: dragging the tab strip/title area did not move the window.
- Cause: the bar only used `-webkit-app-region: drag`, which WKWebView (macOS) and WebView2 ignore. Tauri 2
  moves the window from elements marked `data-tauri-drag-region`; its injected `drag.js` handles mousedown →
  `start_dragging` and double-click → `internal_toggle_maximize` (on mouseup for macOS, cancelled if the
  mouse moved, like native).
- Files: `src/lib/shell/TabStrip.svelte`, `src/lib/shell/LeftPanel.svelte`, `src/App.svelte`
  (the "show panel" corner), `src/lib/shell/WindowControls.svelte`, `src/styles/base.css` (comment).
- Logic: `data-tauri-drag-region="deep"` on the tab strip, the left-panel top bar and the reopen corner, so
  every empty area in their subtree drags (padding under the traffic lights, `.drag-fill`, gaps between tabs).
  Tauri skips interactive elements automatically (`button`, `[role=tab]`, `[tabindex]`, inputs…), so tabs stay
  clickable and keep the pointer-based reorder, and `+`, close and window-control buttons keep working.
  `WindowControls` is also marked `data-tauri-drag-region="false"`.
- Config: `core:window:allow-start-dragging` was already in `src-tauri/capabilities/default.json`;
  `allow-internal-toggle-maximize` comes with `core:default`. `windows.rs` keeps `TitleBarStyle::Overlay` +
  `hidden_title` on macOS and `decorations(false)` on Windows. No config change needed.
- Decisions: the `.drag-region` CSS class stays (no effect in Tauri, harmless). The card editor's page header
  is not part of the window title bar and was left unchanged.
- Risks / not verified: dragging the window cannot be tested in the browser. Checked: the Tauri 2.12
  `drag.js` source, and the same algorithm run on the live DOM (strip, gaps and `.drag-fill` → drag; tab,
  tab title, close, `+` and panel buttons → no drag). Check the move and double-click zoom on macOS and Windows.

## 4 — Command palette keeps the `>` prefix

- Objective: opening the palette in command mode selected the `>`, so the first keystroke replaced it and
  command results were lost.
- Files: `src/lib/quickinput/caret.ts` (new), `src/lib/quickinput/QuickInput.svelte`,
  `src/lib/quickinput/qi.svelte.ts` (`PickOptions.selectAll`), `src/lib/quickinput/palette.ts`.
- Logic: `initialSelection(value, selectAll)` returns the selection range for a freshly opened input (whole
  value by default, caret at end for `selectAll: false`); `QuickInput` applies it with `setSelectionRange`.
  `openPalette` passes `selectAll: false`, so the caret sits after the mode prefix.
- Decisions: `inputBox` / other `quickPick` calls keep select-all for pre-filled values (rename flows rely on
  it). Quick open (empty value) is unchanged. The help entries that reopen the palette with `#` / `@` get
  the same fix, because they go through `openPalette` too.
- Tests: `caret.test.ts`; in the browser, `palette.commands` opens with value `>` and selection 1..1, typing
  `toggle` lists the commands; `inputBox({ value })` still selects 0..len.

## 5 — Context menu opens at the cursor, with fade in/out

- Objective: the menu first appeared top-left and slid to the cursor.
- Cause: the menu was positioned with an inline `transform: translate(x, y)`, and the `pop-in` keyframes
  animate `transform` too, so during the animation the menu was drawn from (0, 0).
- Files: `src/lib/components/ContextMenu.svelte`, `src/lib/components/menuPlacement.ts` (new).
- Logic:
  - Position with `left/top` (`transform` is left to the animation). The menu renders with
    `visibility: hidden`, is measured (`offsetWidth/Height`, which ignore the intro scale) and clamped in a
    microtask before the first paint, then shown.
  - `placeMenu()` clamps to the viewport with a 6 px margin. Submenus get a `flipX` (parent's left edge): if
    a submenu would overflow on the right, it opens to the left of its parent.
  - Svelte transitions `menuIn` / `menuOut` (`|global`, so submenus fade with their parent): 250 ms opacity
    fade plus the existing slide-up/scale on open; fade only on close. During the fade-out the menu is `inert`
    and the click-catching scrim is gone, so clicks go through.
  - Motion: `appearance.motion = none` → 0 ms; `reduced` or OS `prefers-reduced-motion` → 150 ms fade, no
    movement.
- Tests: `menuPlacement.test.ts` (fit, clamp right/bottom/left/top, oversize, submenu flip and no-room
  case, motion levels, OS reduced-motion). In the browser, the first frame was already at the cursor's x (y
  clamped to the viewport for the tall card menu), opacity went 0 → 1 over about 250 ms and back over about
  250 ms, and a submenu flipped left near the right edge.
- Risks: `openMenu` while a menu is fading out reuses the same element (Svelte reverses the outro).

## Checks

`pnpm lint`, `pnpm check`, `pnpm test`, `pnpm i18n en|es|pt`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## Follow-up: the whole top bar drags the window

**Problem (user report, with screenshot):** only the part of the top bar holding the tabs moved the window. The
empty space to the right of the tabs did not.

**Cause:** `.strip` in `TabStrip.svelte` had `flex-shrink: 0` and no `flex-grow`, so it was only as wide as
its tabs. Its `.drag-fill` had nothing to grow into, and the rest of `.pane-top` was not a drag region.

**Fix:** `.strip` is now `flex: 1 1 auto; min-width: 0`, so it fills the whole top bar and `.drag-fill` takes
up all the empty space. Tabs, the **+** button and the window controls still opt out through `.no-drag` and
Tauri's interactive-element check. When there are many tabs, `.tabs` still scrolls, because its `min-width` is
0.

**Verified (web build):** the strip is as wide as `.pane-top` (744 / 744 px). Hit tests across the bar:
points on tabs land on `.no-drag` elements, and the point at the far right lands on `.drag-fill`, inside the
drag region. Native drag still needs to be tried in the desktop app.
