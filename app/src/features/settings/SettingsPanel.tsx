import { useEffect, useState } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { ipc } from "../../lib/ipc";
import { useSettings, type UiSettings } from "../../lib/settings";
import { play, type SoundEvent } from "../../lib/sound";
import type { KeyStatus, PackInfo, Quota } from "../../lib/types";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grid gap-3">
      <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wider text-muted">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <p className="text-[length:var(--text-sm)] font-medium">{label}</p>
        {hint && <p className="text-[length:var(--text-xs)] text-muted">{hint}</p>}
      </div>
      {children}
    </div>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className="flex items-center gap-0.5 rounded-full bg-inset p-0.5" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={`rounded-full px-2.5 py-1 text-[length:var(--text-xs)] font-medium transition-token ${
            value === o.value ? "bg-accent text-accent-fg" : "text-secondary hover:text-primary"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-5.5 w-10 rounded-full transition-token ${
        checked ? "bg-accent" : "bg-inset border border-edge-strong"
      }`}
    >
      <span
        className={`absolute top-0.5 h-4.5 w-4.5 rounded-full bg-white shadow transition-token ${
          checked ? "left-5" : "left-0.5"
        }`}
      />
    </button>
  );
}

export function SettingsPanel({
  open,
  onClose,
  keyStatus,
  onKeyChange,
}: {
  open: boolean;
  onClose: () => void;
  keyStatus: KeyStatus | null;
  onKeyChange: (s: KeyStatus) => void;
}) {
  const { settings, update } = useSettings();
  const [quota, setQuota] = useState<Quota | null>(null);
  const [packs, setPacks] = useState<PackInfo[]>([]);
  const [newKey, setNewKey] = useState("");
  const [keyError, setKeyError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    ipc.quotaGet().then(setQuota).catch(() => {});
    ipc.packsList().then(setPacks).catch(() => {});
  }, [open]);

  const set = <K extends keyof UiSettings>(k: K) => (v: UiSettings[K]) => update(k, v);

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 bg-black/40 backdrop-blur-[2px]" />
        <Dialog.Popup
          className="fixed left-1/2 top-1/2 max-h-[85vh] w-[min(680px,92vw)] -translate-x-1/2 -translate-y-1/2
                     overflow-y-auto rounded-[length:var(--radius-lg)] border border-edge bg-overlay
                     p-[length:var(--sp-5)] shadow-[var(--shadow-overlay)]"
          aria-label="Settings"
        >
          <div className="mb-4 flex items-center justify-between">
            <Dialog.Title className="text-[length:var(--text-lg)] font-semibold">Settings</Dialog.Title>
            <Dialog.Close
              aria-label="Close settings"
              className="rounded-[length:var(--radius-sm)] px-2 py-1 text-secondary transition-token hover:bg-inset hover:text-primary"
            >
              ✕
            </Dialog.Close>
          </div>

          <div className="grid gap-[length:var(--sp-5)]">
            <Section title="Appearance">
              <Row label="Theme">
                <Segmented
                  label="Theme"
                  value={settings.theme}
                  onChange={set("theme")}
                  options={[
                    { value: "system", label: "System" },
                    { value: "porcelain", label: "Porcelain" },
                    { value: "graphite", label: "Graphite" },
                    { value: "midnight", label: "Midnight" },
                    { value: "contrast", label: "Contrast" },
                  ]}
                />
              </Row>
              <Row label="Accent" hint="Hue only — contrast is guaranteed on every hue">
                <div className="flex items-center gap-2">
                  <input
                    type="range"
                    min={0}
                    max={359}
                    value={settings.accentHue}
                    aria-label="Accent hue"
                    onChange={(e) => update("accentHue", Number(e.target.value))}
                    className="w-40 accent-[var(--accent)]"
                  />
                  <span
                    aria-hidden
                    className="h-5 w-5 rounded-full border border-edge-strong"
                    style={{ background: "var(--accent)" }}
                  />
                </div>
              </Row>
              <Row label="Density">
                <Segmented
                  label="Density"
                  value={settings.density}
                  onChange={set("density")}
                  options={[
                    { value: "compact", label: "Compact" },
                    { value: "cozy", label: "Cozy" },
                    { value: "comfortable", label: "Comfortable" },
                  ]}
                />
              </Row>
            </Section>

            <Section title="Typography">
              <Row label="Text size" hint={`${Math.round(settings.fontScale * 100)}%`}>
                <input
                  type="range"
                  min={0.85}
                  max={1.3}
                  step={0.05}
                  value={settings.fontScale}
                  aria-label="Text size"
                  onChange={(e) => update("fontScale", Number(e.target.value))}
                  className="w-40 accent-[var(--accent)]"
                />
              </Row>
              <Row label="Code ligatures">
                <Toggle checked={settings.codeLigatures} onChange={set("codeLigatures")} label="Code ligatures" />
              </Row>
            </Section>

            <Section title="Motion">
              <Row label="Animation intensity" hint="System reduced-motion preference always wins">
                <Segmented
                  label="Animation intensity"
                  value={settings.motion}
                  onChange={set("motion")}
                  options={[
                    { value: "full", label: "Full" },
                    { value: "reduced", label: "Reduced" },
                    { value: "off", label: "Off" },
                  ]}
                />
              </Row>
            </Section>

            <Section title="Sound">
              <Row label="UI sounds" hint="Subtle synthesized cues — off by default">
                <Toggle checked={settings.soundEnabled} onChange={(v) => { update("soundEnabled", v); if (v) setTimeout(() => play("result"), 50); }} label="UI sounds" />
              </Row>
              {settings.soundEnabled && (
                <>
                  <Row label="Volume">
                    <input
                      type="range"
                      min={0}
                      max={1}
                      step={0.05}
                      value={settings.soundVolume}
                      aria-label="Sound volume"
                      onChange={(e) => update("soundVolume", Number(e.target.value))}
                      onMouseUp={() => play("result")}
                      className="w-40 accent-[var(--accent)]"
                    />
                  </Row>
                  {(Object.keys(settings.soundEvents) as SoundEvent[]).map((ev) => (
                    <Row key={ev} label={`· ${ev}`}>
                      <Toggle
                        checked={settings.soundEvents[ev]}
                        onChange={(v) => update("soundEvents", { ...settings.soundEvents, [ev]: v })}
                        label={`Sound on ${ev}`}
                      />
                    </Row>
                  ))}
                </>
              )}
            </Section>

            <Section title="Advisor">
              <Row label="Default depth" hint="Quick ~3 · Balanced ~7 · Deep ~12+ API calls per question">
                <Segmented
                  label="Default depth"
                  value={settings.tier}
                  onChange={set("tier")}
                  options={[
                    { value: "quick", label: "Quick" },
                    { value: "balanced", label: "Balanced" },
                    { value: "deep", label: "Deep" },
                  ]}
                />
              </Row>
              <Row label="Clarifying questions" hint="Ask one question when the symptom is ambiguous between opposite remedies">
                <Toggle checked={settings.clarifyingQuestions} onChange={set("clarifyingQuestions")} label="Clarifying questions" />
              </Row>
            </Section>

            <Section title="API">
              {keyStatus?.present ? (
                <Row label={`Cohere key ·…${keyStatus.last4}`} hint="Stored in Windows Credential Manager">
                  <button
                    type="button"
                    onClick={() => ipc.keyDelete().then(() => onKeyChange({ present: false, last4: null }))}
                    className="rounded-[length:var(--radius-sm)] border border-edge px-3 py-1 text-[length:var(--text-sm)] text-danger transition-token hover:border-danger"
                  >
                    Remove
                  </button>
                </Row>
              ) : (
                <div className="grid gap-2">
                  <Row label="Cohere key" hint="Trial or production">
                    <div className="flex gap-2">
                      <input
                        type="password"
                        value={newKey}
                        onChange={(e) => setNewKey(e.target.value)}
                        placeholder="Paste key"
                        aria-label="Cohere API key"
                        className="w-48 rounded-[length:var(--radius-sm)] border border-edge-input bg-raised px-2 py-1 text-[length:var(--text-sm)]"
                      />
                      <button
                        type="button"
                        disabled={!newKey.trim()}
                        onClick={async () => {
                          setKeyError(null);
                          try {
                            const s = await ipc.keySet(newKey.trim());
                            onKeyChange(s);
                            setNewKey("");
                          } catch (e) {
                            setKeyError((e as { message?: string }).message ?? "Key rejected");
                          }
                        }}
                        className="rounded-[length:var(--radius-sm)] bg-accent px-3 py-1 text-[length:var(--text-sm)] font-medium text-accent-fg disabled:opacity-40"
                      >
                        Save
                      </button>
                    </div>
                  </Row>
                  {keyError && <p className="text-[length:var(--text-xs)] text-danger">{keyError}</p>}
                </div>
              )}
              {quota && (
                <Row
                  label="Usage this month"
                  hint="Trial keys allow 1,000 API calls per month across all endpoints"
                >
                  <span className="text-[length:var(--text-sm)] tabular-nums text-secondary">
                    {quota.chat_calls + quota.embed_calls + quota.rerank_calls} calls
                    <span className="text-muted"> ({quota.chat_calls}c/{quota.embed_calls}e/{quota.rerank_calls}r)</span>
                  </span>
                </Row>
              )}
            </Section>

            <Section title="Knowledge packs">
              {packs.map((p) => (
                <div key={p.pack_id} className="rounded-[length:var(--radius-md)] border border-edge bg-surface p-3">
                  <div className="flex items-center justify-between">
                    <p className="text-[length:var(--text-sm)] font-semibold">{p.name}</p>
                    <span className="text-[length:var(--text-xs)] text-muted">v{p.pack_version}</span>
                  </div>
                  <p className="mt-1 text-[length:var(--text-xs)] text-secondary">{p.description}</p>
                  <p
                    className="mt-2 border-t border-edge pt-2 text-[length:var(--text-xs)] text-muted"
                    dangerouslySetInnerHTML={{ __html: p.attribution_html }}
                  />
                </div>
              ))}
            </Section>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
