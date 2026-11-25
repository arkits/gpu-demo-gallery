#!/usr/bin/env python3
"""Serves dist/ locally, plus the packed demo files under /local/."""
import functools
import http.server
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Exposed, but never copied into dist/. Only these files: a path cannot climb out.
LOCAL = ROOT / "target/package/local"
FILES = {f"/local/{p.relative_to(LOCAL).as_posix()}": p for p in LOCAL.rglob("*") if p.is_file()}


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split("?")[0]
        return str(FILES[path]) if path in FILES else super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
