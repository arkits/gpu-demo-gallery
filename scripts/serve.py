#!/usr/bin/env python3
"""Serves dist/ locally, plus the user's own demo files under /local/."""
import functools
import http.server
import pathlib
import sys
import os

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Exposed, but never copied into dist/.
ARCHIVE = pathlib.Path(os.environ.get("SUSHI_ATI", ROOT / "data/Sushi.ati")).resolve()
LOCAL = {
    "/local/Sushi.ati": ARCHIVE,
    "/local/Audio/SceneIntro.wav": ARCHIVE.parent / "Audio/SceneIntro.wav",
    "/local/Logo2.tga": ARCHIVE.parent / "Logo2.tga",
}



class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split("?")[0]
        return str(LOCAL[path]) if path in LOCAL else super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
