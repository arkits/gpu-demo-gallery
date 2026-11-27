"""Lay out the web routes and split any ZIP above the Pages file-size limit."""
import json
from pathlib import Path
from shutil import copyfile

TRACKS = {
    "nature": ["fallsintro.mp3"],
    "rachel": ["rachelloop.mp3"],
    **{f"island{n}": [f"island{n}intro.mp3", f"island{n}loop.mp3"] for n in range(1, 4)},
}
DEMOS = ("dolphin", "nature", "rachel", "island1", "island2", "island3", "treasure-chest", "geoforms")
dist = Path("dist")
directory = dist / "content"
directory.mkdir(exist_ok=True)
for demo in DEMOS:
    route = dist / demo
    route.mkdir(exist_ok=True)
    copyfile(dist / "index.html", route / "index.html")
    source = Path("data") / f"{demo}.zip"
    parts = []
    with source.open("rb") as pack:
        while chunk := pack.read(20 * 1024 * 1024):
            name = f"{demo}.{len(parts)}.bin"
            (directory / name).write_bytes(chunk)
            parts.append(f"/content/{name}")
    tracks = []
    for name in TRACKS.get(demo, []):
        copyfile(Path("data") / demo / name, directory / name)
        tracks.append(f"/content/{name}")
    (directory / f"{demo}.json").write_text(json.dumps({"parts": parts, "tracks": tracks}) + "\n")
(dist / "404.html").write_text("<!doctype html><title>Demo unavailable</title><p>Demo unavailable.</p>\n")
