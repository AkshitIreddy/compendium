// Subtle UI sound: synthesized cues on a single AudioContext — no assets, no
// licenses, one cohesive voice. Master toggle is OFF by default; every event
// can be disabled individually. Volumes stay low by design (peak gain 0.12).

export type SoundEvent = "send" | "result" | "error" | "toggle";

interface Cue {
  /** [frequency, startOffsetSeconds][] — tiny arpeggios, one oscillator each */
  notes: [number, number][];
  duration: number;
  type: OscillatorType;
}

const CUES: Record<SoundEvent, Cue> = {
  send: { notes: [[520, 0], [780, 0.06]], duration: 0.14, type: "sine" },
  result: { notes: [[523, 0], [659, 0.07], [784, 0.14]], duration: 0.3, type: "sine" },
  error: { notes: [[330, 0], [262, 0.09]], duration: 0.22, type: "triangle" },
  toggle: { notes: [[600, 0]], duration: 0.07, type: "sine" },
};

let ctx: AudioContext | null = null;
let master: GainNode | null = null;

function ensureContext(): { ctx: AudioContext; master: GainNode } {
  if (!ctx) {
    ctx = new AudioContext();
    master = ctx.createGain();
    master.gain.value = 0.5;
    master.connect(ctx.destination);
  }
  return { ctx: ctx!, master: master! };
}

export interface SoundPrefs {
  enabled: boolean;
  volume: number; // 0..1
  events: Record<SoundEvent, boolean>;
}

export const DEFAULT_SOUND_PREFS: SoundPrefs = {
  enabled: false,
  volume: 0.5,
  events: { send: true, result: true, error: true, toggle: false },
};

let prefs: SoundPrefs = DEFAULT_SOUND_PREFS;

export function configureSound(next: SoundPrefs) {
  prefs = next;
  if (master) master.gain.value = next.volume;
}

export function play(event: SoundEvent) {
  if (!prefs.enabled || !prefs.events[event]) return;
  try {
    const { ctx, master } = ensureContext();
    if (ctx.state === "suspended") void ctx.resume();
    const cue = CUES[event];
    const now = ctx.currentTime;
    for (const [freq, offset] of cue.notes) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = cue.type;
      osc.frequency.value = freq;
      // short attack, exponential release — soft, never clicky
      gain.gain.setValueAtTime(0, now + offset);
      gain.gain.linearRampToValueAtTime(0.12, now + offset + 0.012);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + offset + cue.duration);
      osc.connect(gain).connect(master);
      osc.start(now + offset);
      osc.stop(now + offset + cue.duration + 0.02);
    }
  } catch {
    // audio is never worth an error surface
  }
}
