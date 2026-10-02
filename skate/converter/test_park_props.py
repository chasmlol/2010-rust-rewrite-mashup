import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from park_props import extract, selected_entries


class ParkPropsTests(unittest.TestCase):
    def test_selection_includes_models_textures_and_recipe_data_only(self):
        entries = [
            SimpleNamespace(path="data/content/dynamic/model/dynamic/dmo_rails/ramp.rx2"),
            SimpleNamespace(path="data/content/static/texture/wood.rx2"),
            SimpleNamespace(path="data/content/recipe/dynamic/dynamic_db.xml"),
            SimpleNamespace(path="data/content/worlddmo.big"),
            SimpleNamespace(path="data/audio/sound.big"),
        ]

        self.assertEqual(
            [entry.path for entry in selected_entries(entries)],
            [entry.path for entry in entries[:3]],
        )

    def test_extract_writes_local_inventory_and_hashes_without_game_data_mutation(self):
        class Archive:
            entries = [
                SimpleNamespace(path="data/content/dynamic/model/dynamic/dmo_rails/ramp.rx2"),
                SimpleNamespace(path="data/content/recipe/dynamic/objects.xml"),
            ]

            def __init__(self, path):
                self.path = path

            def extract_entries(self, entries, output):
                for entry in entries:
                    target = output.joinpath(*entry.path.split("/"))
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(entry.path.encode("utf-8"))

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            game = root / "game"
            (game / "data/content").mkdir(parents=True)
            (game / "data/content/parkassets.big").write_bytes(b"fixture")
            assets = root / "assets"
            result = extract(game, assets, lambda _: None, archive_factory=Archive)

            self.assertEqual(result["status"], "source-assets-only")
            self.assertEqual(len(result["assets"]), 2)
            self.assertEqual(result["assets"][0]["kind"], "model")
            self.assertTrue((assets / "private/park-props/source-manifest.json").is_file())
            self.assertEqual(
                (game / "data/content/parkassets.big").read_bytes(),
                b"fixture",
            )

    def test_missing_archive_is_reported_as_unavailable(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = extract(Path(temporary), Path(temporary) / "assets", lambda _: None)
            self.assertEqual(result["status"], "unavailable")
            self.assertIn("parkassets.big", result["reason"])


if __name__ == "__main__":
    unittest.main()
