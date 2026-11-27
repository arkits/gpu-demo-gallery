#!/usr/bin/env python3
"""Package original data files from the extracted ATI and NVIDIA archives."""
from argparse import ArgumentParser
from pathlib import Path
from shutil import copyfile
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

DEMO_DIRS = {
    "dolphin": "Dolphin",
    "nature": "Nature",
    "rachel": "Rachel",
    "island1": "Island1",
    "island2": "Island2",
    "island3": "Island3",
    "treasure-chest": "TreasureChest",
}


def package(output, root, sources, extensions, method=ZIP_DEFLATED):
    with ZipFile(output, "w", method) as pack:
        for file in sorted(sources):
            if file.is_file() and file.suffix.lower() in extensions:
                pack.write(file, file.relative_to(root).as_posix().lower())


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("ati", type=Path, help="path to ATI Demos containing the seven demo folders")
    parser.add_argument("geoforms", type=Path, help="path to the extracted GeoForms directory")
    parser.add_argument("--output", type=Path, default=Path("data"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for slug, name in DEMO_DIRS.items():
        root = args.ati / name
        output = args.output / f"{slug}.zip"
        if slug == "treasure-chest":
            if not (root / "Pof").is_dir():
                parser.error(f"missing POF directory under {root}")
            package(output, root, root.rglob("*"), (".pof", ".bmp", ".tga", ".pth", ".sha"))
        else:
            archive = root / "Sushi.ati"
            paths = root / "FlyPaths"
            if not archive.is_file() or not paths.is_dir():
                parser.error(f"missing Sushi.ati or FlyPaths under {root}")
            # Keep the original ZIP intact, including its compressed animations.
            package(output, root, [archive, *paths.iterdir()], (".ati", ".pth"), ZIP_STORED)
            audio = [file for file in (root / "Audio").glob("*") if file.suffix.lower() == ".mp3"]
            if audio:
                folder = args.output / slug
                folder.mkdir(exist_ok=True)
                for track in audio:
                    copyfile(track, folder / track.name.lower())
    geo = args.geoforms
    if not (geo / "models").is_dir():
        parser.error(f"missing models directory under {geo}")
    directories = ("textures", "models", "shaders", "stencils")
    package(args.output / "geoforms.zip", geo, (f for name in directories for f in (geo / name).iterdir()),
            (".tga", ".vol", ".nma", ".fp20", ".vp20", ".fp30", ".vp30"))


if __name__ == "__main__":
    main()
