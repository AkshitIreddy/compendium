# LangSmith docs scope for the v1 knowledge pack

Research date: 2026-07-13. All URLs, counts, and byte sizes below were fetched and verified live on this date. Companion to `research/docs-acquisition.md` (which established the sitemap-scoped `.md` fetch method for the LangChain/LangGraph OSS pages); this report extends the same pack to `/langsmith/` and corrects one prior assumption about `llms.txt`.

## 1. /langsmith/ URL inventory (verified from sitemap.xml)

`https://docs.langchain.com/sitemap.xml` contains **1,441 URLs total; 1,011 under `/langsmith/`**. The LangSmith tree is a **flat namespace** (one slug per page, no topic sub-paths) except for three machine-generated sub-trees:

| Segment | URLs | What it is |
|---|---|---|
| `/langsmith/smith-api/**` | 515 | Generated REST API reference (one page per endpoint) |
| `/langsmith/agent-server-api/**` | 63 | Generated Agent Server API reference |
| `/langsmith/fleet/**` | 24 | "Fleet" no-code agents product |
| `/langsmith/<slug>` (flat docs pages) | **409** | The actual documentation |

Because the namespace is flat, **URL-prefix filtering cannot scope by topic** — the allowlist must be an exact-slug list. The official topic structure lives in the site navigation (`src/docs.json` in the langchain-ai/docs repo), which groups the 409 flat pages as:

| Nav section | ~Pages | Advisor relevance |
|---|---|---|
| Test > Evaluation (concepts, datasets, evaluators, experiments, tutorials, annotation) | ~72 | **YES — core** |
| Test > Prompt engineering + Context Hub | ~23 | **YES** (minus playground/model-config plumbing) |
| Test > Studio (local agent debugging UI) | 5 | Optional tier-2 |
| Monitor > Trace (setup, 38 per-vendor `trace-with-*` integrations, manual instrumentation, config) | ~72 | **Partial** — concepts/instrumentation yes; vendor integration pages no |
| Monitor > Debug (viewing/filtering traces, query syntax, data formats) | ~19 | **YES** (minus bulk export) |
| Monitor > Observe (dashboards, alerts, online evaluators, automations) | ~9 | **YES** |
| Monitor > Engine | 5 | No (managed infra product) |
| Deploy (agent-server, cloud/self-hosted deployment, sandboxes) | ~114 | No |
| Platform (self-hosting ~63 incl. 15 Terraform pages, govern/admin/auth/billing ~45, no-code agents ~25) | ~130 | No |

**Per-page `.md` fetch verified:** all **127** pages in the proposed allowlist below were fetched as `https://docs.langchain.com/langsmith/<slug>.md` — every one returned `200`, `Content-Type: text/markdown`, clean markdown with headings/code fences intact and the same small injected "Documentation Index" preamble seen on OSS pages (strip it, as already planned). Samples: `evaluation-concepts.md` 21,570 B; `evaluate-rag-tutorial.md` 56,127 B; `llm-as-judge.md` 6,242 B; `trace-with-langchain.md` 34,142 B; `pytest.md` 19,579 B.

## 2. Proposed allowlist (127 pages, 1.68 MB measured)

All pages verified present in the sitemap and fetched successfully. Total measured size: **1,681,453 bytes (~1.6 MB), average 12.9 KB/page**, largest `evaluate-complex-agent` (64 KB). At the pack's ~3-4 KB chunk size this is roughly **400-600 chunks** — comfortably within the 2-10k corpus budget.

All slugs are relative to `https://docs.langchain.com/langsmith/`.

**Evaluation — core concepts (5):** `test-overview`, `evaluation`, `evaluation-quickstart`, `evaluation-concepts`, `evaluation-approaches`

**Evaluation — datasets & data formats (6):** `manage-datasets`, `manage-datasets-in-application`, `manage-datasets-programmatically`, `example-data-format`, `dataset-json-types`, `dataset-transformations`

**Evaluation — running evals (4):** `evaluate-llm-application`, `evaluate-with-opentelemetry`, `run-evaluation-from-playground`, `run-evals-api-only`

**Evaluation — evaluator types (11):** `evaluation-types`, `llm-as-judge`, `llm-as-judge-sdk`, `code-evaluator-ui`, `code-evaluator-sdk`, `composite-evaluators-ui`, `composite-evaluators-sdk`, `summary`, `evaluate-pairwise`, `evaluators`, `manage-evaluators-sdk`

**Evaluation — techniques & experiment configuration (18):** `define-target-function`, `evaluate-on-intermediate-steps`, `langchain-runnable`, `evaluate-graph`, `multi-turn-simulation`, `trajectory-evals`, `multiple-scores`, `metric-type`, `experiment-configuration`, `evaluation-async`, `repetition`, `handle-model-rate-limiting`, `bind-evaluator-to-dataset`, `evaluate-existing-experiment`, `local`, `read-local-experiment-results`, `evaluate-with-retry`, `evaluate-with-attachments`

**Evaluation — improving judges (2):** `improve-judge-evaluator-feedback`, `create-few-shot-evaluators`

**Evaluation — test frameworks (3):** `openevals`, `pytest`, `vitest-jest`

**Evaluation — tutorials (5):** `evaluate-chatbot-tutorial`, `evaluate-rag-tutorial`, `test-react-agent-pytest`, `evaluate-complex-agent`, `run-backtests-new-agent`

**Evaluation — analyzing experiments (5):** `analyze-an-experiment`, `compare-experiment-results`, `filter-experiments-ui`, `fetch-perf-metrics-experiment`, `upload-existing-experiments`

**Evaluation — annotation & human feedback (7):** `annotation-queues`, `annotation-queues-sdk`, `assertions`, `set-up-feedback-criteria`, `annotate-traces-inline`, `audit-evaluator-scores`, `attach-user-feedback`

**Online evaluation & automations (6):** `online-evaluations-llm-as-judge`, `online-evaluations-multi-turn`, `online-evaluations-code`, `online-evaluations-composite`, `rules`, `webhooks`

**Prompt engineering (13):** `prompt-engineering`, `prompt-engineering-quickstart`, `prompt-engineering-concepts`, `create-a-prompt`, `manage-prompts`, `manage-prompts-programmatically`, `prompt-template-format`, `use-tools`, `multimodal-content`, `write-prompt-with-ai`, `optimize-classifier`, `prompt-commit`, `multiple-messages`

**Context engineering (2):** `context-engineering-concepts`, `context-hub`

**Observability — core (4):** `observability`, `observability-concepts`, `observability-quickstart`, `observability-llm-tutorial`

**Tracing — instrumentation (9):** `annotate-code`, `trace-with-api`, `log-llm-trace`, `log-retriever-trace`, `ls-metadata-parameters`, `upload-files-with-traces`, `trace-with-langchain`, `trace-with-langgraph`, `trace-with-opentelemetry`

**Tracing — configuration & troubleshooting (15):** `log-traces-to-project`, `trace-without-env-vars`, `conditional-tracing`, `sample-traces`, `distributed-tracing`, `access-current-span`, `serverless-environments`, `log-multimodal-traces`, `trace-generator-functions`, `add-metadata-tags`, `mask-inputs-outputs`, `redact-secrets`, `nest-traces`, `troubleshooting-variable-caching`, `troubleshooting`

**Trace debugging & data formats (9):** `view-traces`, `filter-traces-in-application`, `manage-trace`, `export-traces`, `trace-query-syntax`, `query-threads`, `threads`, `run-data-format`, `feedback-data-format`

**Monitoring (3):** `dashboards`, `alerts`, `insights`

### Deliberate exclusions (~880 URLs)

- `smith-api/**` (515) and `agent-server-api/**` (63): endpoint-by-endpoint API reference — noise for a method advisor.
- Deploy / Agent Server (~114 flat pages: `deploy-*`, `agent-server*`, `cloud`, `kubernetes`, sandboxes, `cron-jobs`, checkpointer/store config, streaming/threads *runtime* pages, etc.): platform deployment, not debugging/eval methodology.
- Self-hosting (~63: `self-host-*`, `*-self-hosted`, Terraform x15, ClickHouse scripts) and Platform/Govern/Admin (~45: `billing`, `rbac`, `abac`, `sso`, `audit-logs`, `usage-and-billing`, `cost-tracking`, `evaluator-spend`, org management): pure admin noise.
- No-code agents / Fleet (`fleet/**` x24 + ~25 flat), LLM Gateway (`llm-gateway*` x8), Engine (x5): separate products.
- 35 vendor-specific `trace-with-*` / `trace-*` integration pages (OpenAI, CrewAI, Cursor, voice, etc.): kept only `trace-with-langchain`, `trace-with-langgraph`, `trace-with-opentelemetry`, consistent with the pack's LangChain/LangGraph focus. Cheap to add later if the advisor gets questions about tracing other frameworks.
- Studio (5 pages) and `openevals`' siblings `harbor-integrations`, `presigned-feedback-tokens`, playground/model-config plumbing (`playground-model-providers`, `managing-model-configurations`, `custom-openai-compliant-model`, `custom-endpoint`): borderline; excluded from v1, flagged as tier-2 candidates.

## 3. License / redistribution status: CLEAR (same as OSS pages)

Verified via the GitHub contents API: **`langchain-ai/docs` contains `src/langsmith/` with 437 entries** — the full source of every published `/langsmith/` flat page (409 sitemap pages + redirect stubs + `agent-server-openapi.json`). The LangSmith docs are in the **same MIT-licensed repo** as the OSS docs (LICENSE: MIT, "Copyright (c) 2025 LangChain", verified previously and unchanged). `robots.txt` remains permissive (`Content-Signal: ai-train=yes, search=yes, ai-input=yes`).

Conclusion: offline redistribution of the LangSmith docs pages in a free non-commercial app carries the same low risk as the OSS pages. Ship the MIT license text + attribution ("Documentation (c) LangChain, MIT License, from docs.langchain.com") in the pack — one notice covers both sections.

## 4. Overlap with existing evaluation notebooks: mostly complementary

Checked all 127 fetched pages: **zero mentions of RAGAS, GroUSE, or deepeval** — LangSmith docs teach evaluation through their own SDK plus `openevals`. So overlap is conceptual, not tooling-level.

**Duplication zone (expect near-duplicate retrieval hits):**
- `evaluate-rag-tutorial` builds correctness / relevance / groundedness / retrieval-relevance judges — the same four metrics as the corpus's `define_evaluation_metrics` (hand-rolled judges) and `evaluation_deep_eval` (GEval versions). Same conceptual content, third implementation.
- `llm-as-judge` / `llm-as-judge-sdk` overlap the judge-construction guidance in `define_evaluation_metrics`.

**Complementary — genuinely new capability for the advisor:**
- **Datasets & experiments infrastructure** (versioned datasets, experiments, regression comparison, backtesting, `pytest`/`vitest` CI harnesses) — the corpus notebooks score ad-hoc; none cover managed experiment tracking.
- **Online evaluation** (scoring live production traffic, sampling, automation rules) — absent from the corpus entirely.
- **Judge improvement loop** (`improve-judge-evaluator-feedback`, `create-few-shot-evaluators`, human annotation queues) — complements `evaluation_grouse` / `end-2-end_rag_evaluation`'s judge-validation theme with a practical workflow.
- **Agent/trajectory evaluation** (`trajectory-evals`, `multi-turn-simulation`, `evaluate-graph`, `evaluate-on-intermediate-steps`) — corpus eval notebooks are RAG-answer-centric; these cover agent behavior.
- **Observability/tracing** — no corpus coverage at all.

Mitigation for the duplication zone: keep both (the advisor benefits from "here's the concept" + "here's the managed-platform version"), but cross-link the LangSmith eval pages to the `define_evaluation_metrics` / `evaluation_deep_eval` technique cards in the ontology so the dossier presents them as alternatives, not independent findings.

## 5. Volatility and completeness cross-checks

- **lastmod is deploy-noise at scale:** 241 of the 409 flat pages carry `lastmod` timestamps within the *same second* (`2026-07-13T15:01:1x`), i.e. a single deploy re-stamped 59% of the section. Genuine edit dates spread back to 2026-04-23. Verdict: identical to the OSS finding — use sitemap lastmod only as a *candidate* filter and gate re-embedding on SHA-256 of the normalized body.
- **Edit velocity is high:** the last 30 commits touching `src/langsmith/` span only 2026-07-07 → 2026-07-13 (4-15 commits/day). The LangSmith section churns faster than the OSS retrieval pages (product UI screenshots, new integrations). Content-hash gating makes this cheap to absorb.
- **URL reorganization risk — moderate:** the section shows active product renaming (LangGraph Platform → "Agent Server", new "Engine", "Fleet", "Context Hub" products; `*-link` redirect stubs in the source). The eval/observability/prompt-engineering slugs we allowlist are the most stable, but the standard guardrails (fail loudly if any allowlisted URL 404s or matched count drops >10%) are mandatory here.
- **llms.txt is NOT a completeness cross-check — correction to prior research.** `https://docs.langchain.com/llms.txt` (99,999 bytes) now ends with: *"Note: this index was truncated to stay under 100,000 characters; 527 pages and 3 OpenAPI specs omitted."* It omits 70 of the 409 flat LangSmith pages (everything alphabetically late: all `trace-with-*`, `trajectory-evals`, `view-traces`, `vitest-jest`, ...) and also lists 12 stubs that are not in the sitemap. Do not use it for reconciliation.
- **Better cross-check:** compare the sitemap's flat-page set against the GitHub contents listing of `src/langsmith/` (437 entries; diff modulo `*-link` stubs and non-`.mdx` files), and/or against `llms-full.txt` `Source:` markers, as already planned for OSS.

## 6. Recommendation

**Acquisition: identical to the LangChain/LangGraph method — sitemap-scoped per-page `.md` fetch — with one deviation: exact-slug allowlist instead of URL-prefix allowlist** (the flat namespace makes prefixes useless). Keep the allowlist as a checked-in list of 127 slugs grouped as in §2; the fetch/strip/front-matter/hash pipeline is unchanged and already proven against these exact URLs (127/127 returned 200 text/markdown today).

**Scope: the 127-page allowlist above** (~1.68 MB, ~400-600 chunks): 72 evaluation + 6 online-eval/automation + 15 prompt/context engineering + 31 observability/tracing/debugging + 3 monitoring. Tier-2 candidates if gaps show up in use: Studio (5), remaining `trace-with-*` vendors (35), playground/model-config (4).

**Refresh cadence: same monthly rebuild as the OSS pages, one shared pipeline run.** Despite the higher raw churn (4-15 commits/day on `src/langsmith/`), content-hash gating means only genuinely changed pages re-embed; monthly keeps the dossier current enough for methodology content. Guardrails per run: (a) any allowlisted slug 404s → hard fail; (b) sitemap flat-page count for `/langsmith/` moves ±10% → human review; (c) reconcile against the GitHub `src/langsmith/` listing, **not** llms.txt (truncated). Version as part of the same `langchain-docs-YYYY.MM.N` pack with the single MIT attribution notice.

## Sources

- https://docs.langchain.com/sitemap.xml (1,441 URLs; 1,011 /langsmith/; per-URL lastmod; fetched 2026-07-13)
- https://docs.langchain.com/langsmith/<slug>.md — all 127 allowlisted pages fetched 200 text/markdown, 1,681,453 B total (incl. evaluation-concepts, evaluate-rag-tutorial, llm-as-judge, observability-concepts, prompt-engineering-concepts, trace-with-langchain, pytest)
- https://docs.langchain.com/llms.txt (99,999 B; explicit truncation notice: "527 pages and 3 OpenAPI specs omitted")
- https://api.github.com/repos/langchain-ai/docs/contents/src and /contents/src/langsmith (437 entries; MIT repo)
- https://raw.githubusercontent.com/langchain-ai/docs/main/src/docs.json (official navigation used for topic grouping)
- https://api.github.com/repos/langchain-ai/docs/commits?path=src/langsmith (30 commits over 2026-07-07..13)
- research/docs-acquisition.md (prior verified findings: .md endpoints, robots.txt Content-Signal, MIT LICENSE)
- research/corpus/catalog.md (existing evaluation notebooks: define_evaluation_metrics, evaluation_deep_eval, evaluation_grouse, end-2-end_rag_evaluation, open-rag-eval-example)
