#!/usr/bin/env python3
"""Serves dist/ locally, plus the packed demo files under /local/."""
import functools
import http.server
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Exposed, but never copied into dist/.
PACK = ROOT / "target/package/box-of-smoke.xz"


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split("?")[0]
        return str(PACK) if path == "/local/box-of-smoke.xz" else super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
