"""Recipe loading: each pack directory contains a recipe.toml describing the
pack identity, source type, license/attribution, embedding, and index params.
The source_type selects the processor (see processors/__init__.py).
"""
from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class EmbeddingSpec:
    model: str = "embed-v4.0"
    dims: int = 1024
    input_type: str = "search_document"


@dataclass
class IndexSpec:
    connectivity: int = 24
    expansion_add: int = 384
    expansion_search: int = 192
    quantization: str = "f16"
    recall_gate: float = 0.98


@dataclass
class Recipe:
    pack_dir: Path
    id: str
    version: str
    name: str
    description: str
    source_type: str
    license_id: str
    license_text: str
    attribution_html: str
    embedding: EmbeddingSpec = field(default_factory=EmbeddingSpec)
    index: IndexSpec = field(default_factory=IndexSpec)
    source: dict = field(default_factory=dict)
    processor_options: dict = field(default_factory=dict)

    @property
    def curation_dir(self) -> Path:
        return self.pack_dir / "curation"


def load_recipe(pack_dir: str | Path) -> Recipe:
    pack_dir = Path(pack_dir)
    recipe_path = pack_dir / "recipe.toml"
    if not recipe_path.exists():
        raise FileNotFoundError(f"no recipe.toml in {pack_dir}")
    with open(recipe_path, "rb") as f:
        raw = tomllib.load(f)

    pack = raw["pack"]
    lic = raw["license"]
    for key in ("id", "text", "attribution_html"):
        if not lic.get(key, "").strip():
            raise ValueError(f"recipe license.{key} is required and must be non-empty")

    return Recipe(
        pack_dir=pack_dir,
        id=pack["id"],
        version=pack["version"],
        name=pack["name"],
        description=pack["description"],
        source_type=pack["source_type"],
        license_id=lic["id"],
        license_text=lic["text"],
        attribution_html=lic["attribution_html"],
        embedding=EmbeddingSpec(**raw.get("embedding", {})),
        index=IndexSpec(**raw.get("index", {})),
        source=raw.get("source", {}),
        processor_options=raw.get("processor", {}),
    )
