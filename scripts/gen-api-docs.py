#!/usr/bin/env python3
"""Generate Markdown API docs for each workspace crate.

For each `crates/<crate>/`, scans every `.rs` file under `src/` for:
  - the crate-level `//!` doc-comment (from `src/lib.rs`)
  - public items (`pub fn|struct|enum|trait|mod|type|const|static|union`)
    and the `///` doc-comment immediately preceding each.

Emits `docs/api/<crate>.md` (one per crate) and `docs/api/index.md`.
Heavy lifting (signatures, generics, trait impls, fully-rendered docs)
is intentionally deferred to docs.rs — every page links there.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

CRATES = [
    "arcp",
    "arcp-core",
    "arcp-client",
    "arcp-runtime",
    "arcp-tower",
    "arcp-axum",
    "arcp-actix-web",
    "arcp-otel",
]
KIND_ORDER = ["mod", "struct", "enum", "trait", "fn", "type", "const", "static", "union"]
KIND_HEADINGS = {
    "mod": "Modules", "struct": "Structs", "enum": "Enums",
    "trait": "Traits", "fn": "Functions", "type": "Type Aliases",
    "const": "Constants", "static": "Statics", "union": "Unions",
}
PUB_ITEM_RE = re.compile(
    r"^\s*pub(?:\s*\([^)]*\))?\s+(?:async\s+|unsafe\s+|const\s+|extern(?:\s+\"[^\"]+\")?\s+)*"
    r"(?P<kind>fn|struct|enum|trait|mod|type|const|static|union)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)


def first_line(text: str) -> str:
    for line in text.splitlines():
        s = line.strip()
        if s:
            return s
    return ""


def extract_crate_doc(lib_rs: Path) -> str:
    if not lib_rs.exists():
        return ""
    lines = []
    for raw in lib_rs.read_text(encoding="utf-8").splitlines():
        s = raw.lstrip()
        if s.startswith("//!"):
            lines.append(s[3:].lstrip() if not s.startswith("//!!") else s[3:])
        elif s == "" and lines:
            lines.append("")
        elif lines:
            break
    return "\n".join(lines).rstrip()


def extract_items(src_dir: Path) -> list[dict]:
    items: list[dict] = []
    for path in sorted(src_dir.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        doc_buf: list[str] = []
        for line in lines:
            stripped = line.lstrip()
            if stripped.startswith("///") and not stripped.startswith("////"):
                doc_buf.append(stripped[3:].lstrip())
                continue
            m = PUB_ITEM_RE.match(line)
            if m and not stripped.startswith("//"):
                summary = first_line("\n".join(doc_buf))
                items.append({
                    "kind": m.group("kind"),
                    "name": m.group("name"),
                    "summary": summary,
                    "file": path.relative_to(src_dir.parent).as_posix(),
                })
                doc_buf = []
                continue
            if not stripped.startswith("#["):
                doc_buf = []
    # de-dup by (kind, name) keeping first occurrence with non-empty summary preference
    seen: dict[tuple[str, str], dict] = {}
    for it in items:
        key = (it["kind"], it["name"])
        if key not in seen or (not seen[key]["summary"] and it["summary"]):
            seen[key] = it
    return list(seen.values())


def render_crate_md(crate: str, cargo_toml: dict, src_dir: Path) -> str:
    pkg = cargo_toml.get("package", {})
    description = pkg.get("description", "")
    crate_doc = extract_crate_doc(src_dir / "lib.rs")
    items = extract_items(src_dir)

    out: list[str] = []
    out.append(f"# `{crate}`")
    out.append("")
    if description:
        out.append(f"> {description}")
        out.append("")
    out.append(f"**Full API reference:** [docs.rs/{crate}](https://docs.rs/{crate})")
    out.append("")
    if crate_doc:
        out.append("## Overview")
        out.append("")
        out.append(crate_doc)
        out.append("")

    grouped: dict[str, list[dict]] = {k: [] for k in KIND_ORDER}
    for it in items:
        grouped.setdefault(it["kind"], []).append(it)

    total = sum(len(v) for v in grouped.values())
    if total == 0:
        out.append("## Public items")
        out.append("")
        out.append("_This crate re-exports another crate and exposes no items of its own._")
        out.append("")
        return "\n".join(out).rstrip() + "\n"

    out.append("## Public items")
    out.append("")
    for kind in KIND_ORDER:
        bucket = sorted(grouped.get(kind, []), key=lambda i: i["name"])
        if not bucket:
            continue
        out.append(f"### {KIND_HEADINGS[kind]}")
        out.append("")
        for it in bucket:
            line = f"- `{it['name']}`"
            if it["summary"]:
                line += f" — {it['summary']}"
            out.append(line)
        out.append("")
    return "\n".join(out).rstrip() + "\n"


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    out_dir = root / "docs" / "api"
    out_dir.mkdir(parents=True, exist_ok=True)

    index_rows: list[tuple[str, str]] = []
    for crate in CRATES:
        crate_dir = root / "crates" / crate
        cargo_path = crate_dir / "Cargo.toml"
        if not cargo_path.exists():
            print(f"skip: {crate} (no Cargo.toml)", file=sys.stderr)
            continue
        cargo = tomllib.loads(cargo_path.read_text(encoding="utf-8"))
        md = render_crate_md(crate, cargo, crate_dir / "src")
        (out_dir / f"{crate}.md").write_text(md, encoding="utf-8")
        index_rows.append((crate, cargo.get("package", {}).get("description", "")))
        print(f"wrote docs/api/{crate}.md")

    index: list[str] = []
    index.append("# Rust SDK — API Reference")
    index.append("")
    index.append(
        "Per-crate summaries of public items. Generated from each crate's "
        "`src/` by `scripts/gen-api-docs.py`. Full signatures, generics, and "
        "rendered doc-comments live on [docs.rs](https://docs.rs)."
    )
    index.append("")
    index.append("## Crates")
    index.append("")
    for crate, desc in index_rows:
        suffix = f" — {desc}" if desc else ""
        index.append(f"- [`{crate}`]({crate}.md){suffix}")
    index.append("")
    (out_dir / "index.md").write_text("\n".join(index), encoding="utf-8")
    print("wrote docs/api/index.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
