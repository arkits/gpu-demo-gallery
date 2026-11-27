#!/usr/bin/env python3
"""Serves dist/ locally, plus the user's own demo files under /local/."""
import functools
import http.server
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Exposed, but never copied into dist/.
LOCAL = {f"/local/Audio/CarRoom{n}.wav": ROOT / f"data/Audio/CarRoom{n}.wav" for n in ("Intro", "Loop")}
LOCAL["/local/carpaint.xz"] = ROOT / "target/package/carpaint.xz"


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split("?")[0]
        return str(LOCAL[path]) if path in LOCAL else super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
