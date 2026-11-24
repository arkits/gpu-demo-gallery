#!/usr/bin/env python3
"""Serves dist/ locally, plus the user's own demo files under /local/."""
import functools
import http.server
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEMO = ROOT / "ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream"
# Only these are exposed, and they are never copied into dist/.
LOCAL = {
    f"/local/{n}": DEMO / n for n in ("Sushi.ati", "Logo2.tga", "Audio/AnimusicIntro.mp3")
}

LOCAL["/local/pipedream.xz"] = ROOT / "target/package/pipedream.xz"


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split("?")[0]
        return str(LOCAL[path]) if path in LOCAL else super().translate_path(path)


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
handler = functools.partial(Handler, directory=str(ROOT / "dist"))
print(f"http://127.0.0.1:{port}/", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
