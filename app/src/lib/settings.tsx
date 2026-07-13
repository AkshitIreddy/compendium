// UI settings context: every value applies live (data-* attributes / CSS vars
// on <html>) and persists through the settings table.
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { ipc } from "./ipc";
import type { Tier } from "./types";
import { configureSound, DEFAULT_SOUND_PREFS, type SoundEvent } from "./sound";

export interface UiSettings {
  theme: "system" | "porcelain" | "graphite" | "midnight" | "contrast";
  accentHue: number; // OKLCH hue 0-360
  accentChroma: number; // clamped so every theme keeps AA
  density: "compact" | "cozy" | "comfortable";
  motion: "full" | "reduced" | "off";
  fontScale: number; // 0.85 - 1.3
  codeLigatures: boolean;
  soundEnabled: boolean;
  soundVolume: number; // 0-1
  soundEvents: Record<SoundEvent, boolean>;
  sourcePanelSide: "right" | "bottom";
  tier: Tier;
  clarifyingQuestions: boolean;
}

export const DEFAULTS: UiSettings = {
  theme: "system",
  accentHue: 275,
  accentChroma: 0.13,
  density: "cozy",
  motion: "full",
  fontScale: 1,
  codeLigatures: false,
  soundEnabled: false,
  soundVolume: 0.5,
  soundEvents: DEFAULT_SOUND_PREFS.events,
  sourcePanelSide: "right",
  tier: "balanced",
  clarifyingQuestions: true,
};

function resolveTheme(theme: UiSettings["theme"]): string {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "graphite"
    : "porcelain";
}

export function applyToDocument(s: UiSettings) {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(s.theme);
  root.dataset.density = s.density;
  root.dataset.motion = s.motion;
  root.style.setProperty("--accent-h", String(s.accentHue));
  // Chroma clamp: keeps user accents inside the range the contrast tests
  // verify (see design/contrast.test.ts — same bound).
  root.style.setProperty("--accent-c", String(Math.min(Math.max(s.accentChroma, 0), 0.16)));
  root.style.setProperty("--font-scale", String(s.fontScale));
  root.style.setProperty("--code-ligatures", s.codeLigatures ? "normal" : "none");
  configureSound({ enabled: s.soundEnabled, volume: s.soundVolume, events: s.soundEvents });
}

interface SettingsContextValue {
  settings: UiSettings;
  update: <K extends keyof UiSettings>(key: K, value: UiSettings[K]) => void;
}

const SettingsContext = createContext<SettingsContextValue>({
  settings: DEFAULTS,
  update: () => {},
});

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<UiSettings>(DEFAULTS);

  useEffect(() => {
    ipc
      .settingsGetAll()
      .then((all) => {
        const stored = (all["ui"] ?? {}) as Partial<UiSettings>;
        const advisor = (all["advisor"] ?? {}) as { tier?: Tier; clarifying_questions?: boolean };
        const merged: UiSettings = {
          ...DEFAULTS,
          ...stored,
          tier: advisor.tier ?? DEFAULTS.tier,
          clarifyingQuestions: advisor.clarifying_questions ?? DEFAULTS.clarifyingQuestions,
        };
        setSettings(merged);
        applyToDocument(merged);
      })
      .catch(() => applyToDocument(DEFAULTS));

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSettings((s) => ({ ...s })); // re-resolve system theme
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    applyToDocument(settings);
  }, [settings]);

  const update = useCallback(
    <K extends keyof UiSettings>(key: K, value: UiSettings[K]) => {
      setSettings((prev) => {
        const next = { ...prev, [key]: value };
        const { tier, clarifyingQuestions, ...ui } = next;
        void ipc.settingsSet("ui", ui);
        void ipc.settingsSet("advisor", {
          tier,
          clarifying_questions: clarifyingQuestions,
        });
        return next;
      });
    },
    [],
  );

  const value = useMemo(() => ({ settings, update }), [settings, update]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings() {
  return useContext(SettingsContext);
}
