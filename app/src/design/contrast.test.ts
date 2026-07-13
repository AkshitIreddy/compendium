/** WCAG AA contrast matrix over every theme's token pairs — parsed from
 * tokens.css itself so the test can never drift from the shipped values.
 * Fails the build at < 4.5:1 for text pairs and < 3:1 for UI pairs.
 *
 * The accent chroma clamp mirrored in lib/settings.tsx (0..0.16) is validated
 * here across the full hue wheel, so no user accent choice can break AA.
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { oklch, wcagContrast, type Oklch } from "culori";

const css = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "tokens.css"),
  "utf-8",
);

const THEMES = ["porcelain", "graphite", "midnight", "contrast"] as const;
const ACCENT_DEFAULT = { h: 275, c: 0.13 };

function parseOklch(body: string, accentC: number, accentH: number): Oklch | null {
  const resolved = body
    .replaceAll("var(--accent-c)", String(accentC))
    .replaceAll("var(--accent-h)", String(accentH))
    .split("/")[0] // ignore alpha
    .trim();
  const [l, c, h] = resolved.split(/\s+/).map(Number);
  if ([l, c].some(Number.isNaN)) return null;
  return { mode: "oklch", l, c, h: Number.isNaN(h) ? 0 : h };
}

function themeVars(theme: string): Record<string, Oklch> {
  const block = css.match(
    new RegExp(String.raw`:root\[data-theme="${theme}"\]\s*\{([^}]*)\}`, "s"),
  )?.[1];
  if (!block) throw new Error(`theme ${theme} not found in tokens.css`);
  const vars: Record<string, Oklch> = {};
  // oklch body may contain nested var(...) — match balanced single nesting
  for (const m of block.matchAll(/--([\w-]+):\s*oklch\(((?:[^()]|\([^()]*\))+)\)/g)) {
    const parsed = parseOklch(m[2], ACCENT_DEFAULT.c, ACCENT_DEFAULT.h);
    if (parsed) vars[m[1]] = parsed;
  }
  return vars;
}

/** (foreground, background, minimum ratio, what it is) */
const TEXT_PAIRS: [string, string, number, string][] = [
  ["fg-primary", "bg-app", 7, "body text"],
  ["fg-primary", "bg-surface", 7, "body text on surface"],
  ["fg-primary", "bg-raised", 4.5, "text on raised"],
  ["fg-secondary", "bg-app", 4.5, "secondary text"],
  ["fg-secondary", "bg-surface", 4.5, "secondary on surface"],
  ["fg-secondary", "bg-inset", 4.5, "secondary on inset"],
  ["accent-fg", "accent", 4.5, "accent button label"],
  ["accent-subtle-fg", "accent-subtle", 4.5, "subtle accent chip"],
  ["fg-primary", "code-bg", 4.5, "code text"],
  ["danger", "bg-inset", 4.5, "error text"],
  ["warning", "bg-inset", 4.5, "warning text"],
];

const UI_PAIRS: [string, string, number, string][] = [
  // border-strong is decorative (hover flourish); border-input is the
  // affordance boundary on form controls and must meet non-text contrast.
  ["border-input", "bg-raised", 3, "input border"],
  ["accent", "bg-app", 3, "focus ring / accent UI"],
  ["fg-muted", "bg-app", 3, "muted (non-essential) text"],
];

describe.each(THEMES)("theme %s", (theme) => {
  const vars = themeVars(theme);
  it.each([...TEXT_PAIRS, ...UI_PAIRS])(
    "%s on %s ≥ %s:1 (%s)",
    (fg, bg, min) => {
      const ratio = wcagContrast(vars[fg]!, vars[bg]!);
      expect(
        ratio,
        `${theme}: ${fg} on ${bg} = ${ratio.toFixed(2)}:1, needs ${min}:1`,
      ).toBeGreaterThanOrEqual(min);
    },
  );

  it("user accents cannot break AA anywhere on the hue wheel (clamped chroma)", () => {
    for (let hue = 0; hue < 360; hue += 15) {
      for (const chroma of [0, 0.08, 0.16]) {
        const resolve = (name: string) => {
          const raw = css
            .match(new RegExp(String.raw`:root\[data-theme="${theme}"\]\s*\{([^}]*)\}`, "s"))![1]
            .match(
              new RegExp(String.raw`--${name}:\s*oklch\(((?:[^()]|\([^()]*\))+)\)`),
            )?.[1];
          if (!raw) throw new Error(`missing --${name}`);
          const parsed = parseOklch(raw, chroma, hue);
          if (!parsed) throw new Error(`unparseable --${name}: ${raw}`);
          return parsed;
        };
        const accent = resolve("accent");
        const accentFg = resolve("accent-fg");
        const ratio = wcagContrast(accentFg, accent);
        expect(
          ratio,
          `${theme} accent h=${hue} c=${chroma}: label ${ratio.toFixed(2)}:1`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  });
});

// keep culori's oklch import used (types satisfied under isolatedModules)
void oklch;
