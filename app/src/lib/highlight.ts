// Shiki singleton: core bundle + JS regex engine (no WASM — a strict Tauri
// CSP forbids unsafe-eval), dual-theme output driven by CSS variables, lazy
// language loading with a small preloaded set.
import type { Highlighter } from "shiki";
import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";

let highlighterPromise: Promise<Highlighter> | null = null;
const loadedLangs = new Set<string>(["python", "typescript", "json", "bash", "markdown"]);

export function getHighlighter(): Promise<Highlighter> {
  highlighterPromise ??= createHighlighterCore({
    themes: [import("shiki/themes/github-light-default.mjs"), import("shiki/themes/github-dark-default.mjs")],
    langs: [
      import("shiki/langs/python.mjs"),
      import("shiki/langs/typescript.mjs"),
      import("shiki/langs/json.mjs"),
      import("shiki/langs/bash.mjs"),
      import("shiki/langs/markdown.mjs"),
    ],
    engine: createJavaScriptRegexEngine({ forgiving: true }),
  }) as Promise<Highlighter>;
  return highlighterPromise;
}

const EXTRA_LANGS: Record<string, () => Promise<unknown>> = {
  rust: () => import("shiki/langs/rust.mjs"),
  toml: () => import("shiki/langs/toml.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
  sql: () => import("shiki/langs/sql.mjs"),
  html: () => import("shiki/langs/html.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  javascript: () => import("shiki/langs/javascript.mjs"),
};

export async function highlight(code: string, lang: string): Promise<string> {
  const highlighter = await getHighlighter();
  let language = lang.toLowerCase();
  if (!loadedLangs.has(language)) {
    const loader = EXTRA_LANGS[language];
    if (loader) {
      await highlighter.loadLanguage((await loader()) as never);
      loadedLangs.add(language);
    } else {
      language = "text";
    }
  }
  return highlighter.codeToHtml(code, {
    lang: loadedLangs.has(language) ? language : "text",
    themes: { light: "github-light-default", dark: "github-dark-default" },
    defaultColor: false, // emit --shiki-light/--shiki-dark vars; CSS picks per theme
  });
}
