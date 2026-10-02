import sys
import tempfile
import types
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np

from park_props import extract, prop_model_entries, selected_entries


class ParkPropsTests(unittest.TestCase):
    def test_selection_keeps_source_files_and_targets_dynamic_dropper_models(self):
        entries = [
            SimpleNamespace(
                path="data/content/dynamic/model/dynamic/dmo_rails/ramp.rx2"
            ),
            SimpleNamespace(path="data/content/static/model/static/plaza/bench.rx2"),
            SimpleNamespace(path="data/content/dynamic/texture/rail.rx2"),
            SimpleNamespace(path="data/content/static/texture/wood.rx2"),
            SimpleNamespace(path="data/content/recipe/dynamic/objects.xml"),
            SimpleNamespace(path="data/content/recipe/static/world.xml"),
            SimpleNamespace(path="data/content/worlddmo.big"),
            SimpleNamespace(path="data/audio/sound.big"),
        ]

        self.assertEqual(
            [entry.path for entry in selected_entries(entries)],
            [entries[0].path, entries[2].path, entries[4].path],
        )
        self.assertEqual(prop_model_entries(entries), [entries[0]])

    def test_extract_writes_render_mesh_and_retail_collision_catalog(self):
        class Archive:
            entries = [
                SimpleNamespace(
                    path="data/content/dynamic/model/dynamic/dmo_rails/ramp.rx2"
                ),
                SimpleNamespace(path="data/content/recipe/dynamic/objects.xml"),
            ]

            def __init__(self, path):
                self.path = path

            def extract_entries(self, entries, output):
                for entry in entries:
                    target = output.joinpath(*entry.path.split("/"))
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(entry.path.encode("utf-8"))

            def read(self, entry):
                return b"rx2 fixture"

        mesh = SimpleNamespace(
            name="Ramp",
            material_name="concrete",
            vertices=np.asarray([[0, 0, 0], [1, 0, 0], [0, 0, 1]], dtype=np.float32),
            faces=np.asarray([[0, 1, 2]], dtype=np.uint32),
            uvs=np.asarray([[0, 0], [1, 0], [0, 1]], dtype=np.float32),
            normals=np.asarray([[0, 1, 0]] * 3, dtype=np.float32),
        )
        parsed = SimpleNamespace(meshes=[mesh], materials=[])
        collision_triangle = SimpleNamespace(
            a=(0.0, 0.0, 0.0),
            b=(1.0, 0.0, 0.0),
            c=(0.0, 0.0, 1.0),
        )
        fake_mdl = types.ModuleType("mdl_parser")
        fake_mdl.parse_rx2 = lambda raw: parsed
        fake_collision = types.ModuleType("retail_collision_mesh")
        fake_collision.decode_rx2_clustered_meshes = lambda raw: [
            SimpleNamespace(triangles=[collision_triangle])
        ]

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            game = root / "game"
            (game / "data/content").mkdir(parents=True)
            archive_path = game / "data/content/parkassets.big"
            archive_path.write_bytes(b"fixture archive")
            assets = root / "assets"
            with patch.dict(
                sys.modules,
                {
                    "mdl_parser": fake_mdl,
                    "retail_collision_mesh": fake_collision,
                },
            ):
                result = extract(
                    game, assets, lambda _: None, archive_factory=Archive
                )

            self.assertEqual(result["status"], "models-and-retail-collision")
            self.assertEqual(result["schema"], 3)
            self.assertEqual(len(result["assets"]), 1)
            prop = result["assets"][0]
            self.assertEqual(prop["collision_triangles"], 1)
            self.assertEqual(prop["collision_source"], "retail RX2 RenderWare ClusteredMesh")
            with np.load(assets / prop["model"], allow_pickle=False) as model:
                np.testing.assert_array_equal(model["faces_0"], [[0, 1, 2]])
            with np.load(assets / prop["collision"], allow_pickle=False) as collision:
                np.testing.assert_array_equal(
                    collision["triangles"],
                    [[[0, 0, 0], [1, 0, 0], [0, 0, 1]]],
                )
            runtime_model = assets / "private/park-props" / prop["runtime_model"]
            self.assertEqual(runtime_model.read_bytes()[:8], b"IW4LPM01")
            self.assertEqual(runtime_model.stat().st_size, 128)
            runtime_collision = assets / "private/park-props" / prop["runtime_collision"]
            self.assertEqual(runtime_collision.read_bytes()[:8], b"IW4LPC01")
            self.assertEqual(runtime_collision.stat().st_size, 48)
            self.assertEqual(len(result["props"]), 1)
            self.assertEqual(result["props"][0]["model"], prop["runtime_model"])
            self.assertEqual(result["props"][0]["collision"], prop["runtime_collision"])
            self.assertTrue(
                (assets / "private/park-props/catalog.json").is_file()
            )
            self.assertEqual(
                archive_path.read_bytes(),
                b"fixture archive",
            )

    def test_missing_archive_is_reported_as_unavailable(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = extract(Path(temporary), Path(temporary) / "assets", lambda _: None)
            self.assertEqual(result["status"], "unavailable")
            self.assertIn("parkassets.big", result["reason"])


if __name__ == "__main__":
    unittest.main()
