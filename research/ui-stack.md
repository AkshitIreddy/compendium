# UI Stack Research — Tauri 2 Windows Desktop App (Linear/Raycast-tier craft)

Research date: 2026-07-13. Versions verified via web research as of July 2026.
Target: Tauri 2 on Windows 11 (WebView2/Chromium evergreen), design-token system, 60fps motion, optional UI sound, markdown/code/Jupyter rendering, deep theming, WCAG AA.

---

## 1. Frontend framework — **Recommendation: React 19**

Svelte 5 (runes) and SolidJS win on raw runtime overhead (~2–7 KB runtime vs ~45 KB for React), and both top the js-framework-benchmark charts. But in a desktop webview shipping local assets, bundle size is nearly irrelevant and all three "feel instant" for typical component trees. The decision is won by ecosystem depth in exactly the areas this app needs:

- **Accessible primitives (the deciding factor).** React has three production-grade options; no other framework has one at the same level:
  - **Base UI** — reached **v1.0 stable in December 2025** (35 components, full-time MUI team, monthly releases). Built by the ex-Radix core engineers. As of mid-2026, **shadcn/ui defaults to Base UI for new projects**. This is the recommended primitive layer.
  - **Radix UI** — still widely used and not deprecated, but after the WorkOS acquisition update velocity slowed on complex components (Combobox, multi-select). Fine to consume via existing shadcn components; don't start new work on it.
  - **React Aria (Adobe)** — the most rigorous accessibility engineering available anywhere (40+ patterns, screen-reader-tested, i18n). Heavier API; use its hooks selectively for hard widgets (e.g., listbox/combobox with virtualization) rather than wholesale.
  - Svelte/Solid's best equivalent is **Ark UI / Zag.js** (state-machine-driven, genuinely good and framework-agnostic) — but it is one team's output vs. three funded ecosystems, and its component coverage and a11y test depth trail React Aria and Base UI.
- **Animation:** Motion v12 (framer-motion successor) is React-first — `layout`/`layoutId` shared-element transitions, `AnimatePresence`, springs. Nothing on Svelte/Solid matches it for orchestrated product motion (Svelte's built-in transitions are good but lower-level).
- **Virtualization:** TanStack Virtual (headless, multi-framework but best-documented in React), react-virtuoso (richest batteries-included API: dynamic heights, sticky headers, infinite scroll). Both mature.
- **Markdown/code:** react-markdown v10 and Vercel's Streamdown are React-only. Notebook-rendering prior art (nteract components, react-ipynb-renderer) is React-only.
- **TypeScript DX:** React 19 + React Compiler removes most memoization busywork; TS inference for JSX is best-tested in React tooling. Svelte 5 TS support is now good, Solid's is good but its JSX-that-isn't-React trips tooling occasionally.

Verdict: choose **React 19 + Vite + Base UI primitives (via shadcn/ui) + React Aria hooks for the hardest widgets**. Solid/Svelte would buy performance headroom you won't need and cost you the best a11y/motion/markdown ecosystems, which are precisely the craft areas in scope.

---

## 2. Design-token architecture — **Recommendation: 3-layer CSS custom properties + Tailwind v4 `@theme inline` + build-time contrast tests in CI**

### Token layers
1. **Primitive tokens** (`--gray-100…900`, `--blue-500`) — raw OKLCH values, defined once. Use OKLCH (Tailwind v4's native choice) for perceptually even ramps and predictable accent generation.
2. **Semantic tokens** (`--bg-surface`, `--bg-raised`, `--text-primary`, `--text-muted`, `--border-subtle`, `--accent`, `--accent-fg`, `--focus-ring`) — the only layer components may reference. Each built-in theme is just a block that reassigns these:
   ```css
   :root, [data-theme="light"] { --bg-surface: var(--gray-50); … }
   [data-theme="dark"] { --bg-surface: oklch(0.18 0.01 260); … }
   [data-theme="midnight"] { … }
   ```
3. **Component tokens** where variants need them (`--button-bg`, `--card-radius`).

### Tailwind v4 fit — excellent
Tailwind v4 is CSS-first: `@theme` replaces `tailwind.config.js` and every token becomes a CSS variable that also generates utilities. The key pattern for runtime multi-theming is **`@theme inline`** pointing Tailwind tokens at your semantic variables:
```css
@theme inline {
  --color-surface: var(--bg-surface);
  --color-accent: var(--accent);
}
```
`inline` makes utilities emit `var(--bg-surface)` directly, so swapping `data-theme` / `.dark` on `<html>` re-themes everything with zero rebuild. Light/dark/system: resolve `system` in JS from `matchMedia('(prefers-color-scheme: dark)')` and stamp `data-theme`; also support a `prefers-color-scheme` fallback for first paint.

### Accent, density, type scale
- **User accent:** store hue (or a full OKLCH color); generate the accent ramp at runtime with **culori** (`oklch → interactive states: hover/active/subtle/fg`) and write `--accent-*` vars. Clamp chroma/lightness so every generated accent passes contrast against surfaces (see below). Offer "Windows accent" as one option (Section 7).
- **Density:** `data-density="compact|default|comfortable"` on the root remaps a small set of spacing/size tokens (`--space-unit`, `--control-height`, `--row-py`). Never scale via `transform`.
- **Type scale:** set root `font-size` from a user setting (e.g., 87.5%–125%) and size *everything* in `rem`; this doubles as WCAG 1.4.4 text-resize compliance. Keep a separate `--font-scale-mono` if code should scale independently.

### WCAG AA contrast verification at build/CI time
Don't rely on manual checkers. Because tokens are plain data, enumerate them in a test:
- Maintain a `tokens.ts` (or JSON) source of truth that both generates the CSS and feeds tests.
- A **Vitest suite** iterates every theme × every declared (foreground, background) pair — `text-primary/bg-surface`, `accent-fg/accent`, `text-muted/bg-raised`, focus ring vs adjacent colors — computing WCAG 2.x contrast with **culori** (or `wcag-contrast`) and asserting ≥ 4.5:1 for text, ≥ 3:1 for large text/UI components (WCAG 1.4.11). Fail CI on any regression. This is the Style-Dictionary-style build-time approach recommended in current design-token workflows, and it runs in milliseconds.
- Optionally also compute **APCA** (`apca-w3` npm package) as an advisory (non-failing) column — APCA is more perceptually accurate, especially for dark themes, but WCAG 2.x AA remains the compliance target.
- For the *runtime-generated* accent ramp, run the same clamp/check function in the app (shared code with the test) so no user-chosen accent can produce failing pairs.
- Layer on **axe-core via Playwright** (`@axe-core/playwright`) against the rendered app for everything tokens can't catch (focus order, names/roles, contrast of real composited pixels over vibrancy).

---

## 3. Rendering pipeline (markdown / code / notebooks)

### Markdown — **Recommendation: react-markdown (remark/rehype) for static content; Streamdown if any content streams in**
- **react-markdown v10** remains the standard: remark parse → mdast → hast → React elements, **no `dangerouslySetInnerHTML`**, fully pluggable. Plugins: `remark-gfm` (tables, strikethrough, task lists, autolinks), `remark-math` + `rehype-katex` if math is needed.
- **Streamdown (Vercel, v2.5, March 2026)** is a drop-in react-markdown replacement purpose-built for token-streamed AI output: it auto-completes unterminated bold/links/code fences so partial markdown never renders broken, and ships Shiki-based code blocks, KaTeX, and Mermaid, accepting the same `remarkPlugins`/`rehypePlugins`. If your RAG answers stream, use Streamdown for the answer pane and plain react-markdown for static sources. Caveat: `@streamdown/code` defaults to Shiki's WASM engine (CSP `unsafe-eval` issue reported, vercel/streamdown#384) — configure the JS engine or your own code-block component (below).
- Custom renderers (`components={{ a, img, table, code, … }}`) give you: external links opened via Tauri's opener plugin (`target=_blank` interception), styled tables with `overflow-x` wrappers, copy buttons on code blocks, and heading anchors.

### Syntax highlighting — **Recommendation: Shiki v4 with `createHighlighterCore`, the JavaScript regex engine, lazy per-language imports, and dual-theme CSS variables**
Shiki is v4.2 as of June 2026 and is the definitive choice (TextMate-grammar accuracy, VS Code themes). Bundle strategy:
- **Do not** import the full bundle (~1.2 MB gz) or web bundle (~700 KB gz). Use `shiki/core` (~12 KB) + `createHighlighterCore`.
- **Engine:** `createJavaScriptRegexEngine()` (`shiki/engine/javascript`) — no Oniguruma WASM download, no CSP `unsafe-eval`, supports ~97% of grammars, ideal in a webview. (For even less JS you can use precompiled grammars + `createJavaScriptRawEngine`.)
- **Lazy languages:** register a small eager set (ts/js/json/python/bash/markdown) and dynamically `import('@shikijs/langs/rust')` etc. on first sight of a fence tag, with a plain-text fallback while loading. Shiki caches loaded grammars in the highlighter.
- **Dual themes:** use Shiki's multi-theme output — `codeToHtml(code, { themes: { light: 'github-light', dark: 'github-dark' }, defaultColor: false })` emits `--shiki-light`/`--shiki-dark` CSS variables per token; your theme CSS flips which variable `color:` uses. One highlight pass serves both modes, and custom app themes can pick which Shiki theme pair they map to. Wrap in `react-shiki` or a ~40-line memoized component; highlight long blocks in `requestIdleCallback` or a worker to keep 60fps.

### Sanitization for notebook-derived content
Treat every notebook as untrusted (arbitrary HTML/JS lives in outputs):
- Markdown route: since react-markdown never injects raw HTML by default, plain markdown cells are safe as-is; if you enable `rehype-raw` for embedded HTML in markdown cells, follow it with **`rehype-sanitize`** using a schema extended only with what you need (e.g., KaTeX classes).
- Raw HTML outputs (`text/html` mime bundles): sanitize with **DOMPurify** (`USE_PROFILES: { html: true }`, forbid `style` with `position:fixed`, strip event handlers, `svg` allowed but scripts/foreignObject stripped). For high-risk rich outputs you choose to support anyway, render inside a **sandboxed `<iframe sandbox="">`** (no `allow-scripts`) with a strict `csp` attribute — but prefer simply not rendering script-bearing outputs.
- Keep Tauri's own CSP strict (`default-src 'self'`; no remote origins), which caps blast radius.

### Jupyter notebook viewer (custom, no nbviewer) — pattern
Parse the `.ipynb` JSON (nbformat 4.x) directly — it's just `{ cells: [...] }`; validate `nbformat >= 4` and normalize `source` (string | string[]) to a string. Then render per cell type:
- **markdown cells** → the exact same react-markdown pipeline as answers (consistent typography), with attachment images resolved from `cell.attachments` as data URIs.
- **code cells** → Shiki block (language from `metadata.language_info.name` / kernelspec), execution count in the gutter (`In [n]:`), collapsed-by-default option for long sources.
- **outputs** — iterate `cell.outputs`, dispatch on `output_type`:
  - `stream` → monospace block; convert ANSI escapes with **anser** (or `ansi-to-react`)-style mapping to your theme's 16-color palette; cap rendered lines with an expander.
  - `execute_result` / `display_data` → pick the **richest mime type you support** from `data`, in priority order: `image/png`/`image/jpeg` (base64 `<img>`, `max-width:100%`), `image/svg+xml` (DOMPurify-sanitized, then inline), `text/html` (DOMPurify — this covers pandas DataFrames; optionally detect `<table>` and restyle with your table tokens), `application/json` (pretty-printed, collapsible), `text/latex` (KaTeX), fallback `text/plain`.
  - `error` → traceback through the ANSI converter inside a visually distinct error surface.
- "Selected outputs" for RAG sources: since you control the renderer, render only the cells/outputs your retrieval pipeline cites, with a "view full cell" affordance.
- Virtualize the cell list (TanStack Virtual) for big notebooks; memoize per-cell rendering keyed on cell content hash.
- Prior art worth reading (not depending on): **nteract/outputs** packages and `react-ipynb-renderer` — both demonstrate the mime-bundle dispatch pattern, but both are stale enough that a custom ~500-line viewer over your existing markdown+Shiki stack is cleaner and matches your tokens.

---

## 4. Motion — **Recommendation: Motion v12 (`motion` package) + transform/opacity discipline + CSS View Transitions for page-level swaps + a token-driven motion-intensity control**

- **Library state:** framer-motion became independent in 2025 and is now **Motion v12** (`npm i motion`, `import { motion } from "motion/react"`); framer-motion the package still works but is unmaintained. Hybrid engine: WAAPI/ScrollTimeline hardware-accelerated paths with JS fallback for springs/gestures; ~30M downloads/month; used by Figma/Framer. This is the definitive pick for React.
- **GPU-friendly rules:** animate **only `transform` and `opacity`** (compositor-only); never animate `width/height/top/left/box-shadow` (layout/paint). For shadows, crossfade two pseudo-element layers via opacity. Use `will-change` transiently (Motion sets it for you). Blur/`backdrop-filter` animation is expensive over vibrancy — fade a pre-blurred layer instead. Target 200ms±, standard easing/springs; motion should explain hierarchy (origin-aware scaling from trigger, staggered lists ≤ 40ms/item).
- **Layout & shared elements:** Motion's `layout` prop + `layoutId` give FLIP-based shared-element transitions (e.g., a source card expanding into a detail pane) that still animate only transforms. `AnimatePresence` for exit animations.
- **View Transitions API:** WebView2 tracks Chromium/Edge stable, so **same-document view transitions are fully available (Edge 111+, Baseline as of 2026; `view-transition-name`, `::view-transition-*` pseudo-elements, nested/scoped transitions in current versions)**. Use `document.startViewTransition` for whole-view mode switches (route/panel swaps, theme changes) where Motion's component-level model is overkill; keep Motion for within-view micro-interactions and gestures. Cross-document transitions (Edge 126+) are irrelevant for an SPA.
- **Motion-intensity control + reduced motion:**
  - Wrap the app in `<MotionConfig reducedMotion="user">` so Motion auto-respects `prefers-reduced-motion` (WebView2 surfaces the Windows "animation effects" setting).
  - Add a **global intensity setting** (`off / reduced / full`) stored with the theme prefs: expose `--motion-scale: 0 | .5 | 1` and a `data-motion` root attribute. CSS transitions multiply durations by `calc(var(--duration) * var(--motion-scale))`; a `useMotionPrefs()` hook feeds the same scale into Motion transition durations and swaps springs for tweens/instant at `off`. Gate `startViewTransition` behind the same hook (fall back to instant swap). "System" maps the setting to `prefers-reduced-motion`.
  - Reduced ≠ frozen: keep opacity crossfades, drop movement/scale — this matches WCAG 2.3.3 intent.

---

## 5. Sound — **Recommendation: raw Web Audio API (no library) + Kenney/ObsydianX CC0 sets, off by default**

- **Why not Howler:** Howler's value is cross-browser quirk handling, sprite maps, and HTML5-audio streaming fallback — none of which matter for ~6–10 short cues in a single known engine (WebView2 Chromium). Raw Web Audio is ~40 lines and strictly lower latency.
- **Pattern:** create one `AudioContext` lazily on first user gesture (autoplay policy); at startup `fetch` each local `.ogg`/`.wav` asset and `decodeAudioData` into cached `AudioBuffer`s; play with a fresh `AudioBufferSourceNode → GainNode → destination`. Decoded buffers start in single-digit milliseconds — no perceptible latency in a webview. Route everything through one master `GainNode` bound to a volume token; `context.suspend()` when sounds are disabled. Keep files < 200ms, normalized to sit well below speech loudness, and debounce so rapid events don't machine-gun.
- **Design rules (Raycast-style):** sounds only for *semantic* moments — success, error, completion of long-running work — never hover/click chatter. Ship the toggle **off by default** (or on only for completion events), with per-category toggles next to the motion-intensity setting.
- **Sources (license-safe):**
  - **Kenney — "Interface Sounds" (100 assets) and "UI Audio" (50 assets)** — CC0, the highest-quality free UI sets; no attribution needed. Best starting point.
  - **ObsydianX "Interface SFX Pack 1"** (itch.io) — 200+ confirm/back/cursor/error tones, CC0.
  - **freesound.org** — filter license = CC0 (e.g., GameAudio UI SFX pack); verify per-file license before bundling.
  - **Pixabay sound effects** — royalty-free, no attribution.
  - **Google Material sound resources** — polished, but **CC BY 4.0** (attribution required in your about/credits screen) — usable, second choice.
  - The "shadcn-style" route many polished apps take: buy or commission one tiny bespoke set for uniqueness; otherwise process CC0 picks (pitch/EQ) so your app has a coherent voice.
  - Record license provenance for each shipped file in a `SOUNDS-LICENSES.md`.

---

## 6. Fonts — **Recommendation: bundle Inter Variable (UI) + JetBrains Mono Variable (code); Geist/Geist Mono as the alternate pairing. All are OFL 1.1 — app bundling is explicitly permitted.**

All four candidates are **SIL Open Font License 1.1**, which explicitly allows bundling/embedding/redistribution with software (keep the license text in your distribution; don't rename derivatives with reserved names). No legal differentiator — choose on merit:

| Font | License | Notes |
|---|---|---|
| **Inter (v4.x, variable)** | OFL 1.1 | The de-facto premium-app UI font (Linear, Raycast, GitHub lineage). Variable axes wght + **opsz** (Inter Display sizing built-in); huge glyph set; killer OpenType features: `tnum` tabular figures (tables/timers), `calt`, `ss01`/`cv05` disambiguated glyphs, `zero`. |
| **Geist + Geist Mono** | OFL 1.1 | Vercel's pairing, on Google Fonts; modern/tight aesthetic, variable wght. Slightly smaller glyph/feature coverage than Inter; the mono is handsome but younger than JetBrains Mono. |
| **JetBrains Mono (variable)** | OFL 1.1 | Purpose-built for code: 1.2× taller x-height at same width, 140+ ligatures (make them a user toggle), excellent italics, wide language coverage. The safest premium code font. |
| **IBM Plex (Sans/Mono)** | OFL 1.1 | Distinctive corporate voice; variable versions exist but the family reads "IBM" — pick only if you want that identity. |

**Verdict:** **Inter Variable + JetBrains Mono Variable** — the highest-craft, most battle-tested pairing. If you prefer a single coherent superfamily voice, **Geist + Geist Mono** is the defensible alternative.

**Variable-font strategy:**
- Ship **one variable woff2 per family per style** (Inter roman + italic VF ≈ ~350 KB each; JBM VF ~100 KB) as local `@font-face` with `font-weight: 100 900` ranges — assets load from the app bundle, so no FOUT concerns and no need for `font-display` games; still subset to latin/latin-ext (+ scripts you support) with `pyftsubset`/glyphhanger, **keeping the OpenType features you use** (`tnum`, `calt`, `liga`, `ss*`, `zero`).
- Use weight as a design token (`--font-weight-medium: 520`) — variable fonts allow non-standard weights for optical fine-tuning per theme (slightly heavier in dark mode reads better).
- Enable `font-feature-settings: 'tnum'` on numeric tables; expose code ligatures as a setting mapped to `font-variant-ligatures`.
- Set `font-family` fallbacks ending in `system-ui`/`Segoe UI Variable` and `Consolas` so text renders during first frame.

---

## 7. Windows niceties (Tauri)

- **window-vibrancy crate (tauri-apps/window-vibrancy, v0.7.x, actively maintained — 0.7.1 late 2025):** integrates with Tauri v2 (v0.4 was the Tauri v1 line). On Windows 11 use **`apply_mica` / `apply_tabbed`** (Mica) — stable and cheap since it samples the wallpaper, not live content. **Avoid `apply_acrylic` on Win10/11 pre-22H2-fixes: documented lag on window resize/drag**; if you want in-app translucency layers, fake them with your own semi-transparent surfaces over Mica. Requirements: `"transparent": true` on the window + `html,body{background:transparent}`; provide an opaque fallback theme when vibrancy is unavailable (Win10) or when the user enables reduced transparency. Honor **`prefers-reduced-transparency`** (supported in Chromium/WebView2) and a manual "transparency" toggle by swapping to opaque surface tokens and removing the effect via the same crate.
- **Custom titlebar with accessible window controls:** `decorations: false` + an element with `data-tauri-drag-region` for the drag strip. Build controls as real `<button aria-label="Minimize/Maximize/Close">` elements (focusable, visible focus ring, in the tab order) calling `getCurrentWindow().minimize()/toggleMaximize()/close()`. **Windows 11 Snap Layouts do not appear on a DIY maximize button** — use **`tauri-plugin-decorum`** or **`tauri-plugin-frame`**, which keep native hit-testing so Snap Layout flyouts work on hover, or **tauri-controls** (React/Solid/Svelte components mimicking native Windows caption buttons). Also: double-click drag region = maximize; right-click = system menu (decorum handles both); keep the drag region ≥ 32px and don't cover it with interactive elements.
- **Windows accent color in the webview:** Chromium/WebView2 supports the CSS system colors **`AccentColor` and `AccentColorText`** — read them directly in CSS, or resolve at runtime (paint to canvas or `getComputedStyle` trick) to seed your OKLCH accent ramp so hover/active/subtle variants stay token-driven and contrast-checked. Belt-and-braces fallback: read the accent from Rust (`windows` crate `UISettings::GetColorValue(UIColorType::Accent)` or registry `HKCU\Software\Microsoft\Windows\DWM\AccentColor`) and emit it as a CSS var; also `window.setEffects`/theme events keep Mica in sync with system light/dark. Make "Use Windows accent" one choice in the accent picker, run it through the same AA clamp as user accents.
- **High contrast (Contrast Themes):** WebView2 exposes it as **`forced-colors: active`** — the engine force-maps colors to the user's palette. Do *not* fight it: audit under `@media (forced-colors: active)`, replace box-shadow-only affordances with real borders/outlines, use system color keywords (`CanvasText`, `ButtonText`, `Highlight`, `AccentColor`) where you must re-specify, and apply `forced-color-adjust: none` only to swatches where the literal color *is* the content (theme previews, syntax samples). Test with Windows Contrast Themes (Aquatic/Desert), not just DevTools emulation. Vibrancy should be disabled automatically under forced colors.

---

## 8. Open-source Tauri/desktop apps worth studying

1. **Yaak** (mountain-loop/yaak, MIT) — Tauri 2 + **React** + Rust API client. The closest architectural sibling to this project; its creator (ex-Insomnia founder) explicitly positions **design as the product's main benefit**, and it's the Tauri app most often praised for feel: crisp theming system (many built-in themes + user themes), custom titlebar done right on Windows, fast panes, keyboard-first. Study: theme/token implementation, command palette, Windows titlebar.
2. **Cap** (CapSoftware/Cap) — open-source Loom/Screen Studio alternative, Tauri + Rust + **SolidStart** frontend, Tailwind. Widely cited for "beautiful, shareable" recordings and a polished consumer-grade UI in Tauri; also a good reference for a non-React Tauri frontend and for smooth progress/recording motion design.
3. **Jan** (janhq/jan) — local-AI chat app on Tauri; directly relevant because it renders **streaming markdown + code blocks** in a chat/answer UI like a RAG app, with theming and model management panes. Study: streaming answer rendering, message virtualization, settings UX. *(Honorable mention: **Spacedrive** — historically the flagship "premium Tauri UI" for its custom design system and virtualized explorer; still worth reading for its interface package, but check repo activity before treating it as current best practice.)*

---

## Definitive stack summary

| Area | Pick |
|---|---|
| Framework | React 19 + Vite + TypeScript |
| Primitives | Base UI (via shadcn/ui, 2026 default) + React Aria hooks for hard widgets |
| Styling/tokens | Tailwind v4 CSS-first, 3-layer OKLCH custom properties, `@theme inline`, `data-theme`/`data-density`/`data-motion` root attributes |
| Contrast CI | culori/`wcag-contrast` Vitest matrix over theme×pair tokens (AA hard-fail) + `apca-w3` advisory + axe-core Playwright |
| Markdown | react-markdown v10 (+ Streamdown for streamed answers), remark-gfm, rehype-sanitize |
| Code | Shiki v4 core + JS regex engine, lazy langs, dual-theme CSS vars |
| Notebooks | Custom nbformat-JSON viewer over the same markdown/Shiki stack; DOMPurify for HTML outputs; anser for ANSI; TanStack Virtual |
| Motion | Motion v12 (`motion/react`) + View Transitions API (Baseline in WebView2); transform/opacity only; `MotionConfig reducedMotion="user"` + `--motion-scale` intensity token |
| Sound | Raw Web Audio (AudioBuffer pool, one master gain); Kenney/ObsydianX CC0 sets; off by default |
| Fonts | Inter Variable (UI) + JetBrains Mono Variable (code), OFL 1.1, subset woff2, `tnum` |
| Windows | window-vibrancy 0.7 Mica (not Acrylic), tauri-plugin-decorum/tauri-controls titlebar, `AccentColor` CSS + UISettings fallback, `forced-colors` audit, `prefers-reduced-transparency` |
| Reference apps | Yaak, Cap, Jan (+ Spacedrive's interface package) |

## Key risks
- WebView2 is evergreen: users' runtime versions vary slightly; feature-detect `startViewTransition` and `AccentColor` rather than assuming.
- Acrylic resize lag on Windows is a known window-vibrancy issue — stick to Mica; test vibrancy + virtualized scrolling for compositing cost.
- Radix-based shadcn components are in maintenance-slowdown; prefer Base UI-based generations to avoid a future migration.
- Notebook `text/html` outputs are an XSS vector — sanitization must be on by default and tested with hostile fixtures.
- Streamdown's default Shiki WASM engine conflicts with a strict Tauri CSP — override with the JS engine.
- Spacedrive's maintenance status is uncertain; treat it as a design reference, not a dependency pattern.

## Sources
- https://motion.dev/docs/react ; https://github.com/motiondivision/motion
- https://shiki.style/guide/ ; https://shiki.style/guide/regex-engines ; https://shiki.style/guide/best-performance
- https://github.com/vercel/streamdown ; https://github.com/vercel/streamdown/issues/384
- https://github.com/remarkjs/react-markdown ; https://nbformat.readthedocs.io/en/latest/format_description.html ; https://components.nteract.io/
- https://tailwindcss.com/docs/theme ; https://tailwindcss.com/blog/tailwindcss-v4
- https://www.infoq.com/news/2026/02/baseui-v1-accessible/ ; https://blog.logrocket.com/headless-ui-alternatives/ ; https://www.greatfrontend.com/blog/top-headless-ui-libraries-for-react-in-2026
- https://tanstack.com/virtual/latest ; https://www.pkgpulse.com/guides/tanstack-virtual-vs-react-window-vs-react-virtuoso-2026
- https://caniuse.com/view-transitions ; https://developer.mozilla.org/en-US/docs/Web/API/View_Transition_API ; https://learn.microsoft.com/en-us/microsoft-edge/web-platform/release-notes/142
- https://github.com/tauri-apps/window-vibrancy ; https://v2.tauri.app/learn/window-customization/ ; https://github.com/clearlysid/tauri-plugin-decorum ; https://github.com/agmmnn/tauri-controls
- https://developer.mozilla.org/en-US/docs/Web/CSS/@media/forced-colors ; https://www.smashingmagazine.com/2022/03/windows-high-contrast-colors-mode-css-custom-properties/ ; https://blogs.windows.com/msedgedev/2020/09/17/styling-for-windows-high-contrast-with-new-standards-for-forced-colors/
- https://github.com/vercel/geist-font/blob/main/LICENSE.txt ; https://fonts.google.com/specimen/Geist
- https://kenney.nl/assets/interface-sounds ; https://kenney.nl/assets/ui-audio ; https://obsydianx.itch.io/interface-sfx-pack-1 ; https://freesound.org/people/GameAudio/packs/13940/
- https://github.com/mountain-loop/yaak ; https://github.com/CapSoftware/Cap ; https://github.com/tauri-apps/awesome-tauri
- https://www.alwaystwisted.com/articles/a-design-tokens-workflow-part-16 (build-time token contrast validation)
