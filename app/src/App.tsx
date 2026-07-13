// Phase 3 debug pane — a throwaway harness for the engine, replaced by the
// real advisor UI in Phase 5.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface PackInfo {
  pack_id: string;
  name: string;
  pack_version: string;
  healed: boolean;
}

interface KeyStatus {
  present: boolean;
  last4: string | null;
}

interface SearchResponse {
  dense_used: boolean;
  cards: {
    slug: string;
    title: string;
    stage_id: string;
    complexity: string;
    score: number;
    exact_cosine: number | null;
    expanded_from: [string, string] | null;
  }[];
  chunks: {
    chunk_id: number;
    technique_slug: string | null;
    heading_path: string;
    exact_cosine: number | null;
    score: number;
  }[];
  failure_modes: { id: string; name: string; score: number; best_phrasing: string }[];
}

export default function App() {
  const [packs, setPacks] = useState<PackInfo[]>([]);
  const [keyStatus, setKeyStatus] = useState<KeyStatus>({ present: false, last4: null });
  const [keyInput, setKeyInput] = useState("");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<SearchResponse | null>(null);

  useEffect(() => {
    invoke<PackInfo[]>("packs_list").then(setPacks);
    invoke<KeyStatus>("key_status").then(setKeyStatus).catch(() => {});
    const unlisten = listen("packs-loaded", () => {
      invoke<PackInfo[]>("packs_list").then(setPacks);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function saveKey() {
    setError(null);
    try {
      const status = await invoke<KeyStatus>("key_set", { key: keyInput });
      setKeyStatus(status);
      setKeyInput("");
    } catch (e: unknown) {
      setError(JSON.stringify(e));
    }
  }

  async function runSearch() {
    if (!query.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setResults(await invoke<SearchResponse>("search_query", { query }));
    } catch (e: unknown) {
      setError(JSON.stringify(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="debug">
      <h1>Compendium — engine debug pane</h1>

      <section>
        <h2>Packs</h2>
        {packs.length === 0 && <p>No packs loaded yet…</p>}
        <ul>
          {packs.map((p) => (
            <li key={p.pack_id}>
              <strong>{p.name}</strong> v{p.pack_version}
              {p.healed && <em> (index healed)</em>}
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2>Cohere key</h2>
        {keyStatus.present ? (
          <p>
            Key configured (…{keyStatus.last4}){" "}
            <button
              onClick={() =>
                invoke("key_delete").then(() => setKeyStatus({ present: false, last4: null }))
              }
            >
              remove
            </button>
          </p>
        ) : (
          <p>
            <input
              type="password"
              placeholder="Cohere API key (trial or production)"
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              size={40}
            />{" "}
            <button onClick={saveKey} disabled={!keyInput.trim()}>
              save
            </button>
          </p>
        )}
      </section>

      <section>
        <h2>Search</h2>
        <p>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && runSearch()}
            placeholder="describe a retrieval problem…"
            size={60}
          />{" "}
          <button onClick={runSearch} disabled={busy}>
            {busy ? "searching…" : "search"}
          </button>
        </p>
        {error && <pre className="error">{error}</pre>}
        {results && (
          <div className="results">
            <p>
              <em>
                {results.dense_used ? "hybrid (dense + BM25)" : "local only (BM25 + ontology)"}
              </em>
            </p>
            <h3>Failure modes</h3>
            <ol>
              {results.failure_modes.map((fm) => (
                <li key={fm.id}>
                  <code>{fm.id}</code> {fm.name} ({fm.score.toFixed(3)}) — “{fm.best_phrasing}”
                </li>
              ))}
            </ol>
            <h3>Cards</h3>
            <ol>
              {results.cards.slice(0, 12).map((c) => (
                <li key={c.slug}>
                  <code>{c.slug}</code> [{c.stage_id}/{c.complexity}] rrf={c.score.toFixed(4)}
                  {c.exact_cosine != null && ` cos=${c.exact_cosine.toFixed(3)}`}
                  {c.expanded_from && ` ← ${c.expanded_from[1]} (${c.expanded_from[0]})`}
                </li>
              ))}
            </ol>
            <h3>Chunks</h3>
            <ol>
              {results.chunks.slice(0, 10).map((c) => (
                <li key={c.chunk_id}>
                  #{c.chunk_id} <code>{c.technique_slug}</code> {c.heading_path.slice(0, 70)}
                  {c.exact_cosine != null && ` cos=${c.exact_cosine.toFixed(3)}`}
                </li>
              ))}
            </ol>
          </div>
        )}
      </section>
    </main>
  );
}
