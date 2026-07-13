"""CLI: python -m compendium_pack build packs/rag-techniques --source <clone> [--out ../packs-out]
     python -m compendium_pack validate ../packs-out/rag-techniques.pack
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .builder import build_pack
from .validator import ValidationError, validate_pack


def _read_env_key(name: str) -> str:
    env_path = Path(__file__).resolve().parents[2] / ".env"
    if env_path.exists():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith(f"{name}="):
                value = line.split("=", 1)[1].strip()
                if value:
                    return value
    import os

    value = os.environ.get(name, "")
    if not value:
        sys.exit(f"error: {name} not found in .env or environment")
    return value


def main() -> None:
    parser = argparse.ArgumentParser(prog="compendium_pack")
    sub = parser.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("build", help="build a pack from a recipe directory")
    b.add_argument("pack_dir", type=Path)
    b.add_argument("--source", type=Path, required=True, help="local clone of the source repo/docs")
    b.add_argument("--out", type=Path, default=None, help="output dir (default: <repo>/packs-out)")
    b.add_argument(
        "--key-env",
        default="COHERE_API_KEY_PRODUCTION",
        help="env var / .env entry holding the Cohere key (build uses the production key)",
    )

    v = sub.add_parser("validate", help="validate a built .pack file")
    v.add_argument("pack_file", type=Path)

    args = parser.parse_args()

    if args.cmd == "build":
        out_dir = args.out or Path(__file__).resolve().parents[2] / "packs-out"
        api_key = _read_env_key(args.key_env)
        pack_path = build_pack(args.pack_dir, args.source, out_dir, api_key)
        report = validate_pack(pack_path)
        print("validation: OK")
        print(json.dumps(report["counts"], indent=2))
    elif args.cmd == "validate":
        try:
            report = validate_pack(args.pack_file)
        except ValidationError as e:
            sys.exit(f"validation FAILED:\n{e}")
        print("validation: OK")
        print(json.dumps(report, indent=2, default=str))


if __name__ == "__main__":
    main()
