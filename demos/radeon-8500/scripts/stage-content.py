"""Lay out the web routes and split any ZIP above the Pages file-size limit."""
from html import escape
import json
from pathlib import Path
from shutil import copyfile

TRACKS = {
    "nature": ["fallsintro.mp3"],
    "rachel": ["rachelloop.mp3"],
    **{f"island{n}": [f"island{n}intro.mp3", f"island{n}loop.mp3"] for n in range(1, 4)},
}
DEMOS = {
    "dolphin": "ATI Dolphin",
    "nature": "ATI Nature",
    "rachel": "ATI Rachel",
    "island1": "ATI Island 1",
    "island2": "ATI Island 2",
    "island3": "ATI Island 3",
    "treasure-chest": "ATI Treasure Chest",
    "geoforms": "GeoForms",
}
dist = Path("dist")
directory = dist / "content"
directory.mkdir(exist_ok=True)
template = Path("web/index.html").read_text()
gallery = "https://gpu-demo-gallery.pages.dev/"
(dist / "index.html").write_text(
    f'<!doctype html><meta charset="utf-8"><title>GPU Demo Gallery</title>'
    f'<meta http-equiv="refresh" content="0; url={gallery}">'
    f'<a href="{gallery}">Open the GPU Demo Gallery</a>\n'
)
for demo, title in DEMOS.items():
    route = dist / demo
    route.mkdir(exist_ok=True)
    hardware = "NVIDIA GeForce 7 · 2006" if demo == "geoforms" else "ATI Radeon 8500 · 2001"
    route.joinpath("index.html").write_text(
        template.replace("{{slug}}", demo).replace("{{title}}", escape(title))
        .replace("{{hardware}}", escape(hardware))
    )
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
