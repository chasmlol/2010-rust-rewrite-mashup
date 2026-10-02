"""Converts the Skate 3 data IW4L's skate mode reads, from the player's own
extracted default.xex: animation banks, state graphs, input and physics
settings, and the skater model the board and rig are taken from.

Runs the converters of SK8-ENGINE/skate-3-rust-engine (the `tools` tree of the
same revision as `skate/crates`). Nothing is downloaded and nothing from the
game is bundled.

    iw4l-skate-convert --xex <path to default.xex> --out <folder>

Writes <folder>/assets on success; progress lines go to stdout.
"""
from pathlib import Path
import argparse, json, runpy, shutil, sys, tempfile, traceback

ROOT = Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parent))
sys.path.insert(0, str(ROOT))

REQUIRED = [
    'data/big/miscload.big',
    'data/big/miscboot.big',
    'data/big/db.big',
    'data/content/createacharacter.big',
]


def run_task(script, args):
    # The converters start their own helper scripts through `--task`.
    script = Path(script)
    if not script.is_absolute():
        script = ROOT / script
    script = script.resolve()
    if not script.is_relative_to((ROOT / 'tools').resolve()):
        raise RuntimeError('Invalid conversion script')
    sys.path.insert(0, str(script.parent))
    sys.argv = [str(script), *args]
    runpy.run_path(str(script), run_name='__main__')
    return 0


def convert(xex, out):
    xex = xex.resolve()
    if xex.suffix.lower() == '.iso':
        raise RuntimeError('ISO files are not supported. Extract the disc and select its default.xex.')
    if xex.name.lower() != 'default.xex' or not xex.is_file():
        raise RuntimeError(f'Select default.xex from an extracted Skate 3 (Xbox 360) game folder, not {xex.name}.')
    game = xex.parent
    missing = [path for path in REQUIRED if not (game / path).is_file()]
    if missing:
        raise RuntimeError('This folder is missing Skate 3 game data (' + ', '.join(missing) +
                           '). Keep the data folder beside default.xex.')

    from tools.asset_pipeline import asset_exports as exports

    out = out.resolve()
    stage = out.with_name(out.name + '.partial')
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)

    def report(text):
        print(text, flush=True)

    with tempfile.TemporaryDirectory(prefix='iw4l-skate-', dir=stage.parent) as work, \
            (stage / 'conversion.log').open('w', encoding='utf-8') as log:
        work = Path(work)
        converted = exports.core(game, stage, work, report, log)
        exports.character(game, stage, work, report, log, converted)

    from tools.asset_pipeline import park_props

    park_inventory = park_props.extract(game, assets=stage / 'assets', report=report)
    if park_inventory['status'] == 'unavailable':
        report(f"Create-a-Park assets unavailable: {park_inventory['reason']}")
        availability = stage / 'assets/private/park-props/availability.json'
        availability.parent.mkdir(parents=True, exist_ok=True)
        availability.write_text(json.dumps(park_inventory, indent=2), encoding='utf-8')

    assets = stage / 'assets'
    for needed in ('private/skater.glb', 'private/game.json', 'private/stock/physics-skeletons.json',
                   'private/stock/skater-collections.json'):
        if not (assets / needed).is_file():
            raise RuntimeError(f'Conversion finished without {needed}.')
    shutil.rmtree(out, ignore_errors=True)
    stage.rename(out)
    report('Skate 3 data ready')


def extract_props_only(game_root, assets):
    game_root = game_root.resolve()
    if not game_root.is_dir():
        raise RuntimeError(f'Select the decompiled Skate 3 folder, not {game_root}.')
    archive = game_root / 'data/content/parkassets.big'
    if not archive.is_file():
        raise RuntimeError(
            f'{archive} is missing. Select the game folder containing data\\content\\parkassets.big.'
        )

    assets = assets.resolve()
    assets.parent.mkdir(parents=True, exist_ok=True)
    from tools.asset_pipeline import park_props

    def report(text):
        print(text, flush=True)

    backup = assets / 'private/park-props.previous'
    with tempfile.TemporaryDirectory(prefix='iw4l-props-', dir=assets.parent) as temporary:
        staged_assets = Path(temporary)
        result = park_props.extract(game_root, staged_assets, report)
        if result['status'] != 'models-and-retail-collision' or not result.get('props'):
            raise RuntimeError('No usable Create-a-Park props with collision were extracted.')
        staged_props = staged_assets / 'private/park-props'
        target_props = assets / 'private/park-props'
        target_props.parent.mkdir(parents=True, exist_ok=True)
        shutil.rmtree(backup, ignore_errors=True)
        if target_props.exists():
            target_props.replace(backup)
        try:
            staged_props.replace(target_props)
        except Exception:
            if backup.exists() and not target_props.exists():
                backup.replace(target_props)
            raise
        shutil.rmtree(backup, ignore_errors=True)
    report(f"Installed {len(result['props'])} props in {assets / 'private/park-props'}")


def main():
    if len(sys.argv) > 2 and sys.argv[1] == '--task':
        return run_task(sys.argv[2], sys.argv[3:])
    parser = argparse.ArgumentParser()
    parser.add_argument('--xex', type=Path)
    parser.add_argument('--props-only', action='store_true',
                        help='extract only the local Create-a-Park model and collision catalog')
    parser.add_argument('--game-root', type=Path,
                        help='decompiled game folder containing data/content/parkassets.big')
    parser.add_argument('--out', type=Path, required=True,
                        help='conversion folder, or assets folder with --props-only')
    args = parser.parse_args()
    try:
        if args.props_only:
            if args.game_root is None:
                parser.error('--game-root is required with --props-only')
            extract_props_only(args.game_root, args.out)
        else:
            if args.xex is None:
                parser.error('--xex is required unless --props-only is used')
            convert(args.xex, args.out)
    except Exception as error:
        traceback.print_exc()
        print(f'ERROR: {error}', flush=True)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
