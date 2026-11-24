#!/usr/bin/env python3
"""Serves dist/ locally, plus the user's own Sushi.ati (never copied into dist/)."""
import functools
import http.server
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream/Sushi.ati"


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        if path.split("?")[0] == "/Sushi.ati":
            return str(DATA)
        return super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/")
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
