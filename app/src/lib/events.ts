// App-event abstraction: Tauri events in the real app, window CustomEvents in
// demo mode (?demo=1) so the UI can be driven without the Rust backend —
// used by the gifsmith README demo and harmless everywhere else.
import { listen as tauriListen } from "@tauri-apps/api/event";

export const isDemo =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("demo");

export type Unlisten = () => void;

export async function listenAppEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<Unlisten> {
  if (isDemo) {
    const domHandler = (e: Event) => handler((e as CustomEvent<T>).detail);
    window.addEventListener(`demo:${name}`, domHandler);
    return () => window.removeEventListener(`demo:${name}`, domHandler);
  }
  return tauriListen<T>(name, (e) => handler(e.payload));
}

export function emitDemoEvent<T>(name: string, detail: T) {
  window.dispatchEvent(new CustomEvent(`demo:${name}`, { detail }));
}
