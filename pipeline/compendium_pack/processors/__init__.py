"""Processor registry: each pack's recipe.toml declares a source_type, and the
matching processor owns the raw-sources -> documents+chunks logic for that
type. Adding a new source type = adding a module here with a process()
function and registering it (a normal feature PR; see CONTRIBUTING.md).
"""
from __future__ import annotations

from typing import Callable

from . import notebook

PROCESSORS: dict[str, Callable] = {
    "notebook": notebook.process,
}


def get_processor(source_type: str) -> Callable:
    if source_type not in PROCESSORS:
        raise KeyError(
            f"no processor for source_type '{source_type}' (known: {sorted(PROCESSORS)})"
        )
    return PROCESSORS[source_type]
