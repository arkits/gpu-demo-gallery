"""Make a manifest and split assets to fit Cloudflare Pages."""
import hashlib
import json
from pathlib import Path
import sys

archive = Path(sys.argv[1])
limit = 24 * 1024 * 1024
size = archive.stat().st_size
files = [archive.name]
if size > limit:
    files = []
    with archive.open("rb") as source:
        for index in range((size + limit - 1) // limit):
            part = archive.with_name(f"{archive.stem}-{index:02}.part")
            part.write_bytes(source.read(limit))
            files.append(part.name)
out = archive.with_name("assets.json")
temp = out.with_suffix(".tmp")
# Browsers keep the parts; a new archive must not be mixed with an old one.
version = hashlib.sha256(archive.read_bytes()).hexdigest()[:12]
temp.write_text(json.dumps({"size": size, "version": version, "files": files}) + "\n")
temp.replace(out)
