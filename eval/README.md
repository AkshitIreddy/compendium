# Eval harness

A deliberately small evaluation suite that measures the *quality* of what the advisor
retrieves and writes — as opposed to the test suite, which checks that the machinery
*works*. It exists to demonstrate the concept end-to-end and to be extended later; it is
not (yet) a rigorous benchmark.

Both halves of RAG are scored separately, because either can fail independently:

| Half | What's scored | How | Cost |
|---|---|---|---|
| **Retrieval** | Did hybrid search surface the right technique cards? | Golden set → hit@5, recall@10, MRR | 1 batched embed call |
| **Generation** | Are the advisor's answers grounded and on-topic? | [DeepEval](https://github.com/confident-ai/deepeval) faithfulness + answer relevancy, LLM-as-judge | ~35 calls (dump) + judge calls |

DeepEval was chosen over ragas (stale langchain pin) and TruLens (heavier,
instrumentation-oriented): it's actively maintained, needs no langchain — the judge
talks to Cohere through the plain SDK — and the corpus literally ships an
`evaluation_deep_eval` technique card, so the harness dogfoods a technique the app
recommends.

## Layout

```
eval/
  retrieval_golden.json       12 queries × expected technique slugs (the answer key)
  generation_questions.json   6 realistic user questions for the generation eval
  deepeval_eval.py            DeepEval scoring (faithfulness, answer relevancy)
  requirements.txt            Python deps for deepeval_eval.py (isolated venv)
  out/                        results JSON (gitignored — regenerate to reproduce)
```

The retrieval scorer and the generation dump step live with the other Rust integration
tests: [`app/src-tauri/tests/eval_retrieval.rs`](../app/src-tauri/tests/eval_retrieval.rs).
Both are `#[ignore]`d — like the live advisor test, they spend trial-key API calls and
only run when asked.

## Running it

```bash
# 0. one-time Python setup (isolated from pipeline/.venv)
python -m venv eval/.venv
eval/.venv/Scripts/pip install -r eval/requirements.txt

# 1. retrieval metrics (1 embed call; also enforces regression floors)
cd app/src-tauri
cargo test --test eval_retrieval retrieval_metrics_on_golden_set -- --ignored --nocapture

# 2. generate advisories for the generation eval (~35 trial calls)
cargo test --test eval_retrieval dump_generation_inputs -- --ignored --nocapture

# 3. DeepEval scoring (uses COHERE_API_KEY_PRODUCTION if set, else the trial key)
cd ../..
eval/.venv/Scripts/python eval/deepeval_eval.py
```

## Baseline results (2026-07-14, packs 2026.07.0)

### Retrieval — golden set of 12, real hybrid search (dense + BM25 + RRF + graph)

| Metric | Score | Meaning |
|---|---|---|
| **hit@5** | **1.000** | every query had a correct technique in the top 5 cards |
| **recall@10** | **1.000** | every expected technique appeared in the top 10 |
| **MRR** | **0.674** | the first correct card sits at rank ~1.5 on average |

The test asserts regression floors (hit@5 ≥ 0.75, MRR ≥ 0.55, recall@10 ≥ 0.60) set
below the baseline, so honest drift fails loudly while run-to-run variance doesn't.

### Generation — 6 Balanced advisories scored by DeepEval

| Metric | Score | Meaning |
|---|---|---|
| **Faithfulness** | **0.984** | fraction of answer claims supported by the retrieved evidence |
| **Answer relevancy** | **0.989** | fraction of answer statements that address the question asked |

5 of 6 advisories scored a clean 1.000 on both metrics; the sixth (long-wiki
fragmentation) came in at 0.905/0.933. That's the documents-mode grounding + S8
verification doing their job — the synthesis model *can't* cite what wasn't retrieved.

*(Judge: `command-a-03-2025`, talking to Cohere via the plain SDK — production key when
`COHERE_API_KEY_PRODUCTION` is set, trial key with pacing otherwise. Scores are
LLM-judged and vary slightly run to run.)*

## Design notes & honest limitations

- **The golden set is small and in-scope by construction.** 12 queries over topics the
  packs demonstrably cover. That's the right start (an answer key must contain *correct*
  answers), but it means these numbers say "retrieval works well on covered topics," not
  "coverage is complete." Out-of-scope and adversarial queries are the natural next
  addition.
- **Expected-slug lists are tight, not exhaustive.** When retrieval ranked a technique
  that is genuinely a correct answer but missing from the key (e.g. Self-RAG for "decide
  when to retrieve"), the key was corrected — the standard golden-set curation loop.
- **MRR undercounts here.** Rank-1 cards that aren't in the expected list are often
  *also* defensible answers (the key is tight); MRR treats them as misses-at-rank-1.
- **Generation metrics are reference-free.** Faithfulness and answer relevancy need no
  hand-written ideal answers, which keeps the harness cheap to extend — add a question,
  rerun. Reference-based metrics (context precision vs. a ground truth) are the next
  step up in rigor.
- **Not wired into CI on purpose.** Every run spends live API calls and LLM-judged
  scores are nondeterministic — a bad fit for the installer-only workflow. This is a
  local, on-demand harness.

## Extending it

- Add cases to `retrieval_golden.json` (any pack, any phrasing style — casual ones are
  valuable). Keep expected lists tight and correct them when retrieval surfaces a
  legitimately right answer the key missed.
- Add questions to `generation_questions.json`, rerun steps 2–3. Keep them unambiguous —
  a question that legitimately needs clarification triggers the advisor's clarify path
  and fails the dump step on purpose (the dump disables the clarify *setting*, but an
  empty answer still can't be scored).
- More DeepEval metrics (`ContextualPrecisionMetric`, `ContextualRecallMetric`) become
  available once cases carry an `expected_output` ground-truth answer.
- Per-stage evals (does the sufficiency gate catch planted-insufficient evidence? does
  rerank improve MRR over raw fusion?) are the deepest extension — the trace data
  (`turn_traces`) already records what each stage did.
