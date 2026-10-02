"""Extract local Create-a-Park source assets from the player's parkassets.big."""

import hashlib
import json
from pathlib import Path


ARCHIVE = Path("data/content/parkassets.big")
ROOTS = (
    "data/content/dynamic/model/",
    "data/content/static/model/",
    "data/content/dynamic/texture/",
    "data/content/static/texture/",
    "data/content/recipe/",
)


def selected_entries(entries):
    selected = []
    for entry in entries:
        path = entry.path.replace("\\", "/").lower()
        if path.startswith(ROOTS) and path.endswith(
            (".rx2", ".loc", ".recipe", ".xml", ".txt", ".toc", ".unlocks", ".layout")
        ):
            selected.append(entry)
    return selected


def asset_kind(path):
    path = path.lower()
    if "/model/" in path and path.endswith(".rx2"):
        return "model"
    if "/texture/" in path and path.endswith(".rx2"):
        return "texture"
    if path.endswith(".loc"):
        return "locator"
    return "recipe_metadata"


def extract(game_root, assets, report, archive_factory=None):
    archive_path = Path(game_root) / ARCHIVE
    if not archive_path.is_file():
        return {
            "version": 1,
            "status": "unavailable",
            "reason": f"missing source archive {ARCHIVE.as_posix()}",
            "assets": [],
        }

    if archive_factory is None:
        from tools.owned_game.big import BigArchive

        archive_factory = BigArchive
    archive = archive_factory(archive_path)
    entries = selected_entries(archive.entries)
    models = [entry for entry in entries if asset_kind(entry.path) == "model"]
    if not models:
        raise ValueError(f"{ARCHIVE.as_posix()} contains no Create-a-Park model assets")

    output = Path(assets) / "private" / "park-props" / "source"
    archive.extract_entries(entries, output)
    manifest = []
    for entry in entries:
        source_path = entry.path.replace("\\", "/")
        extracted = output.joinpath(*source_path.split("/"))
        digest = hashlib.sha256(extracted.read_bytes()).hexdigest()
        stem = Path(source_path).stem
        category = next(
            (
                part.removeprefix("dmo_")
                for part in Path(source_path).parts
                if part.lower().startswith("dmo_")
            ),
            "static" if "/static/" in source_path.lower() else "uncategorized",
        )
        manifest.append({
            "id": hashlib.sha256(source_path.lower().encode("utf-8")).hexdigest()[:16],
            "name": f"{category} {stem}",
            "kind": asset_kind(source_path),
            "source": source_path,
            "file": extracted.relative_to(Path(assets)).as_posix(),
            "bytes": extracted.stat().st_size,
            "sha256": digest,
        })

    result = {
        "version": 1,
        "status": "source-assets-only",
        "source_archive": ARCHIVE.as_posix(),
        "assets": manifest,
    }
    manifest_path = Path(assets) / "private" / "park-props" / "source-manifest.json"
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = manifest_path.with_suffix(".json.new")
    temporary.write_text(json.dumps(result, indent=2), encoding="utf-8")
    temporary.replace(manifest_path)
    report(f"Extracted {len(models)} Create-a-Park model assets and {len(entries) - len(models)} supporting files")
    return result
