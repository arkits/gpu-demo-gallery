#!/usr/bin/env python3
"""Serves dist/ locally, plus the packed demo files under /local/."""
import functools
import http.server
import pathlib
import sys
from urllib.parse import unquote

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Exposed, but never copied into dist/.
LOCAL = ROOT / "target/package"


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split("?")[0]
        name = pathlib.PurePosixPath(path).name
        if path == f"/local/{name}" and name.endswith((".xz", ".flac", ".json", ".part")):
            decoded = unquote(name)
            if "/" not in decoded and "\\" not in decoded:
                return str(LOCAL / decoded)
        return super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
