"""Extract local Create-a-Park source assets from the player's parkassets.big."""

import hashlib
import json
import struct
from pathlib import Path


ARCHIVE = Path("data/content/parkassets.big")
ROOTS = (
    "data/content/dynamic/model/",
    "data/content/dynamic/texture/",
    "data/content/recipe/dynamic/",
)
PROP_MODEL_ROOT = "data/content/dynamic/model/dynamic/"


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


def prop_model_entries(entries):
    return [
        entry
        for entry in entries
        if entry.path.replace("\\", "/").lower().startswith(PROP_MODEL_ROOT)
        and "/dmo_" in entry.path.replace("\\", "/").lower()
        and entry.path.lower().endswith(".rx2")
    ]


def _write_model_archive(output, model):
    import numpy as np

    arrays = {}
    meshes = []
    for index, mesh in enumerate(model.meshes):
        vertices = np.asarray(mesh.vertices, dtype=np.float32)
        faces = np.asarray(mesh.faces, dtype=np.uint32)
        if vertices.ndim != 2 or vertices.shape[1] != 3:
            raise ValueError(f"mesh {index} has invalid vertex dimensions")
        if faces.ndim != 2 or faces.shape[1] != 3:
            raise ValueError(f"mesh {index} has invalid face dimensions")
        if not len(vertices) or not len(faces) or int(faces.max()) >= len(vertices):
            raise ValueError(f"mesh {index} has empty or out-of-range geometry")
        if not np.isfinite(vertices).all():
            raise ValueError(f"mesh {index} contains non-finite vertices")

        arrays[f"vertices_{index}"] = vertices
        arrays[f"faces_{index}"] = faces
        if mesh.uvs is not None:
            uvs = np.asarray(mesh.uvs, dtype=np.float32)
            if uvs.shape != (len(vertices), 2) or not np.isfinite(uvs).all():
                raise ValueError(f"mesh {index} has invalid UVs")
            arrays[f"uvs_{index}"] = uvs
        if mesh.normals is not None:
            normals = np.asarray(mesh.normals, dtype=np.float32)
            if normals.shape != (len(vertices), 3) or not np.isfinite(normals).all():
                raise ValueError(f"mesh {index} has invalid normals")
            arrays[f"normals_{index}"] = normals

        minimum = vertices.min(axis=0)
        maximum = vertices.max(axis=0)
        meshes.append({
            "index": index,
            "name": mesh.name,
            "material": mesh.material_name,
            "vertices": len(vertices),
            "triangles": len(faces),
            "bounds": {
                "minimum": minimum.tolist(),
                "maximum": maximum.tolist(),
            },
        })
    if not meshes:
        raise ValueError("model contains no render meshes")
    np.savez_compressed(output, **arrays)
    return meshes


def _write_collision_archive(output, clustered_meshes):
    import numpy as np

    triangles = [
        [triangle.a, triangle.b, triangle.c]
        for mesh in clustered_meshes
        for triangle in mesh.triangles
    ]
    if not triangles:
        return 0
    values = np.asarray(triangles, dtype=np.float32)
    if values.ndim != 3 or values.shape[1:] != (3, 3) or not np.isfinite(values).all():
        raise ValueError("decoded collision has invalid triangle geometry")
    np.savez_compressed(output, triangles=values)
    return len(values)


def _write_runtime_model(output, model):
    import numpy as np

    with output.open("wb") as stream:
        stream.write(b"IW4LPM01")
        stream.write(struct.pack("<I", len(model.meshes)))
        for index, mesh in enumerate(model.meshes):
            vertices = np.asarray(mesh.vertices, dtype="<f4")
            faces = np.asarray(mesh.faces, dtype="<u4")
            if vertices.ndim != 2 or vertices.shape[1] != 3:
                raise ValueError(f"mesh {index} has invalid vertex dimensions")
            if faces.ndim != 2 or faces.shape[1] != 3:
                raise ValueError(f"mesh {index} has invalid face dimensions")
            if not len(vertices) or not len(faces) or int(faces.max()) >= len(vertices):
                raise ValueError(f"mesh {index} has empty or out-of-range geometry")
            if not np.isfinite(vertices).all():
                raise ValueError(f"mesh {index} contains non-finite vertices")
            normals = (
                np.asarray(mesh.normals, dtype="<f4")
                if mesh.normals is not None
                else np.zeros_like(vertices)
            )
            uvs = (
                np.asarray(mesh.uvs, dtype="<f4")
                if mesh.uvs is not None
                else np.zeros((len(vertices), 2), dtype="<f4")
            )
            if normals.shape != vertices.shape or not np.isfinite(normals).all():
                raise ValueError(f"mesh {index} has invalid normals")
            if uvs.shape != (len(vertices), 2) or not np.isfinite(uvs).all():
                raise ValueError(f"mesh {index} has invalid UVs")
            stream.write(struct.pack("<II", len(vertices), faces.size))
            for position, normal, uv in zip(vertices, normals, uvs):
                stream.write(struct.pack("<8f", *position, *normal, *uv))
            stream.write(faces.tobytes(order="C"))
    return output.stat().st_size


def _write_runtime_collision(output, clustered_meshes):
    import numpy as np

    triangles = [
        [triangle.a, triangle.b, triangle.c]
        for mesh in clustered_meshes
        for triangle in mesh.triangles
    ]
    if not triangles:
        return 0
    values = np.asarray(triangles, dtype="<f4")
    if values.ndim != 3 or values.shape[1:] != (3, 3) or not np.isfinite(values).all():
        raise ValueError("decoded collision has invalid triangle geometry")
    with output.open("wb") as stream:
        stream.write(b"IW4LPC01")
        stream.write(struct.pack("<I", len(values)))
        stream.write(values.tobytes(order="C"))
    return len(values)


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
    models = prop_model_entries(entries)
    if not models:
        raise ValueError(
            f"{ARCHIVE.as_posix()} contains no Create-a-Park dynamic prop models"
        )

    import sys

    converter_root = Path(__file__).resolve().parents[1]
    vendor = converter_root / "vendor"
    sys.path.insert(0, str(vendor / "utt"))
    sys.path.insert(
        0, str(vendor / "university/tools/vanilla_map_extraction/tools")
    )
    import mdl_parser
    from retail_collision_mesh import decode_rx2_clustered_meshes

    root = Path(assets) / "private" / "park-props"
    source_root = root / "source"
    archive.extract_entries(entries, source_root)
    manifest = []
    unsupported = []
    for number, entry in enumerate(models):
        source_path = entry.path.replace("\\", "/")
        raw = archive.read(entry)
        identity = hashlib.sha256(source_path.lower().encode("utf-8")).hexdigest()[:16]
        stem = Path(source_path).stem
        category = next(
            part.removeprefix("dmo_")
            for part in Path(source_path).parts
            if part.lower().startswith("dmo_")
        )
        model_path = root / "models" / f"{identity}.npz"
        collision_path = root / "collision" / f"{identity}.npz"
        runtime_model_path = root / "models" / f"{identity}.iw4lmesh"
        runtime_collision_path = root / "collision" / f"{identity}.iw4lcollision"
        model_path.parent.mkdir(parents=True, exist_ok=True)
        collision_path.parent.mkdir(parents=True, exist_ok=True)
        try:
            parsed = mdl_parser.parse_rx2(raw)
            mesh_manifest = _write_model_archive(model_path, parsed)
            clustered = decode_rx2_clustered_meshes(raw)
            collision_count = _write_collision_archive(collision_path, clustered)
            _write_runtime_model(runtime_model_path, parsed)
            if collision_count:
                _write_runtime_collision(runtime_collision_path, clustered)
        except (ValueError, IndexError, KeyError, OSError) as error:
            unsupported.append({"source": source_path, "error": str(error)})
            model_path.unlink(missing_ok=True)
            collision_path.unlink(missing_ok=True)
            runtime_model_path.unlink(missing_ok=True)
            runtime_collision_path.unlink(missing_ok=True)
            continue
        extracted = source_root.joinpath(*source_path.split("/"))
        manifest.append({
            "id": identity,
            "name": f"{category} {stem}",
            "kind": "dynamic_prop",
            "source": source_path,
            "file": extracted.relative_to(Path(assets)).as_posix(),
            "bytes": extracted.stat().st_size,
            "sha256": hashlib.sha256(raw).hexdigest(),
            "model": model_path.relative_to(Path(assets)).as_posix(),
            "runtime_model": runtime_model_path.relative_to(root).as_posix(),
            "meshes": mesh_manifest,
            "collision": (
                collision_path.relative_to(Path(assets)).as_posix()
                if collision_count
                else None
            ),
            "runtime_collision": (
                runtime_collision_path.relative_to(root).as_posix()
                if collision_count
                else None
            ),
            "collision_triangles": collision_count,
            "collision_source": "retail RX2 RenderWare ClusteredMesh",
            "units": "retail model units",
            "materials": [
                {"kind": material.kind, "value": material.value}
                for material in parsed.materials
            ],
        })
        if (number + 1) % 25 == 0:
            report(f"Decoded {number + 1}/{len(models)} Create-a-Park prop models")

    result = {
        "version": 2,
        "schema": 3,
        "status": "models-and-retail-collision",
        "source_archive": ARCHIVE.as_posix(),
        "props": [
            {
                "id": item["id"],
                "name": item["name"],
                "model": item["runtime_model"],
                "collision": item["runtime_collision"],
            }
            for item in manifest
            if item["runtime_collision"]
        ],
        "assets": manifest,
        "unsupported": unsupported,
    }
    if not manifest:
        detail = unsupported[0]["error"] if unsupported else "no supported model geometry"
        raise RuntimeError(f"Could not decode any Create-a-Park props: {detail}")
    if not result["props"]:
        raise RuntimeError(
            "Create-a-Park models were decoded, but none had usable retail collision geometry"
        )
    manifest_path = root / "catalog.json"
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = manifest_path.with_suffix(".json.new")
    temporary.write_text(json.dumps(result, indent=2), encoding="utf-8")
    temporary.replace(manifest_path)
    report(
        f"Decoded {len(manifest)} Create-a-Park prop models; "
        f"{sum(item['collision_triangles'] > 0 for item in manifest)} have retail collision, "
        f"{len(unsupported)} were unsupported"
    )
    return result
