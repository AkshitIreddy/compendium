"""DeepEval generation eval for Compendium advisories.

Scores (question, answer, retrieved contexts) triples — produced by the Rust
dump step (eval_retrieval.rs::dump_generation_inputs) — on two reference-free
metrics, with Cohere's flagship model as the LLM judge (no langchain involved;
the judge talks to Cohere through the plain SDK):

  faithfulness      — is every claim in the answer supported by the contexts?
  answer relevancy  — does the answer actually address the question asked?

Run (after the dump step):
  eval/.venv/Scripts/python eval/deepeval_eval.py

Writes eval/out/deepeval_results.json and prints a per-question table.
"""

import json
import os
import time
from pathlib import Path

from dotenv import load_dotenv

ROOT = Path(__file__).resolve().parents[1]
load_dotenv(ROOT / ".env")
os.environ.setdefault("DEEPEVAL_TELEMETRY_OPT_OUT", "YES")

import cohere  # noqa: E402
from deepeval.metrics import AnswerRelevancyMetric, FaithfulnessMetric  # noqa: E402
from deepeval.models.base_model import DeepEvalBaseLLM  # noqa: E402
from deepeval.test_case import LLMTestCase  # noqa: E402

JUDGE_MODEL = "command-a-03-2025"
# Prefer the production key (fast); fall back to the trial key with pacing
# that stays under its 18-chat-calls/min ceiling.
_PROD_KEY = os.environ.get("COHERE_API_KEY_PRODUCTION", "").strip()
JUDGE_KEY = _PROD_KEY or os.environ["COHERE_API_KEY_TRIAL"]
PACE_SECONDS = 0.5 if _PROD_KEY else 3.5


class CohereJudge(DeepEvalBaseLLM):
    """DeepEval judge backed by the Cohere v2 chat API.

    When DeepEval passes a pydantic schema, the call uses Cohere's native
    json_object response format (never combined with documents/tools — the
    same constraint the app's engine honors) and validates into the schema;
    otherwise it returns plain text and DeepEval parses the JSON out itself.
    """

    def __init__(self) -> None:
        self.client = cohere.ClientV2(api_key=JUDGE_KEY)

    def load_model(self):  # noqa: D102 — DeepEval interface
        return self.client

    @staticmethod
    def _extract_json(text: str) -> str:
        # Plain-text fallback: strip code fences / prose around the JSON body.
        start, end = text.find("{"), text.rfind("}")
        return text[start : end + 1] if start != -1 and end > start else text

    def generate(self, prompt: str, schema=None):
        last_err: Exception | None = None
        for attempt in range(5):
            time.sleep(PACE_SECONDS * (attempt + 1))  # pacing + backoff on retries
            params = {
                "model": JUDGE_MODEL,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.0,
            }
            # First tries use native json_object mode; later retries drop it in
            # case a specific schema shape is what the API is choking on (the
            # judge prompts already instruct JSON output, so plain text parses).
            if schema is not None and attempt < 3:
                params["response_format"] = {
                    "type": "json_object",
                    "schema": schema.model_json_schema(),
                }
            try:
                resp = self.client.chat(**params)
                text = resp.message.content[0].text
                if schema is None:
                    return text
                return schema.model_validate_json(self._extract_json(text))
            except Exception as e:  # transient 5xx/429 or a malformed parse
                last_err = e
                print(f"    judge call failed (attempt {attempt + 1}/5): {type(e).__name__}")
        raise last_err

    async def a_generate(self, prompt: str, schema=None):
        return self.generate(prompt, schema)

    def get_model_name(self) -> str:
        return JUDGE_MODEL


def main() -> None:
    rows = json.loads((ROOT / "eval/out/generation_inputs.json").read_text(encoding="utf-8"))
    judge = CohereJudge()
    metrics = {
        "faithfulness": FaithfulnessMetric(model=judge, include_reason=False, async_mode=False),
        "answer_relevancy": AnswerRelevancyMetric(model=judge, include_reason=False, async_mode=False),
    }
    print(f"scoring {len(rows)} advisories with DeepEval (judge: {JUDGE_MODEL}) ...")

    per_question = []
    for i, r in enumerate(rows):
        case = LLMTestCase(
            input=r["question"],
            actual_output=r["answer"],
            retrieval_context=r["contexts"],
        )
        scores = {}
        for name, metric in metrics.items():
            metric.measure(case)
            scores[name] = round(float(metric.score), 3)
        per_question.append({"question": r["question"], **scores})
        print(f"[{i + 1}/{len(rows)}] {scores}  |  {r['question'][:60]}")

    agg = {
        name: round(sum(q[name] for q in per_question) / len(per_question), 3)
        for name in metrics
    }
    out = {"judge": JUDGE_MODEL, "per_question": per_question, "aggregate": agg}

    (ROOT / "eval/out").mkdir(exist_ok=True)
    (ROOT / "eval/out/deepeval_results.json").write_text(json.dumps(out, indent=2), encoding="utf-8")

    print("-" * 88)
    print(
        f"AGGREGATE:  faithfulness = {agg['faithfulness']:.3f}   "
        f"answer_relevancy = {agg['answer_relevancy']:.3f}"
    )
    print("wrote eval/out/deepeval_results.json")


if __name__ == "__main__":
    main()
