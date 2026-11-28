"""Compiles the Cg programs of NVIDIA's Gas Station with the demo's own Cg runtime, as the
viewer does when it starts (GasStation.exe 0x460130), into the listings the port reads.

    python3 tools/nv_compile.py INSTALL OUT

INSTALL is the demo's folder (with bin/cg.dll); OUT gets <file>.<entry>.asm and index.json
("file|entry|profile" -> listing). The scenes' programs come from
`cargo run --release --example programs -- INSTALL`. cg.dll runs under Unicorn (see
cgdll_emu.py): pip install unicorn pefile. Programs that are already assembly are copied.
"""
import concurrent.futures as cf
import json
import os
import subprocess
import sys

here = os.path.abspath(os.path.dirname(__file__))
install, out = (os.path.abspath(a) for a in sys.argv[1:3])
bindir = os.path.join(install, 'bin')
os.makedirs(out, exist_ok=True)
prof = {6146: 'vp20', 6147: 'fp20', 6148: 'vp30', 6149: 'fp30'}
listed = subprocess.run(['cargo', 'run', '-q', '--release', '--example', 'programs', '--', install],
                        cwd=os.path.join(here, '..'), check=True, capture_output=True, text=True).stdout
programs = [json.loads(line) for line in listed.splitlines() if line.strip()]
jobs, index = [], {}


def add(fn, entry, p):
    key = f"{fn}|{entry}|{p}"
    if key in index:
        return
    name = fn.replace('\\', '/').split('/')[-1]
    index[key] = f"{name}.{entry}.asm"
    jobs.append((fn, entry, p, os.path.join(out, index[key])))


for s in programs:
    add(s['filename'], s['entry'], prof[s['profile']])
# The viewer's built-in programs (0x4561a0…0x456a20), from its shaders folder.
for name, p in [('_scr.vp20', 'vp20'), ('_mvp.vp20', 'vp20'), ('_scr_tex.vp20', 'vp20'), ('_white.fp30', 'fp30'),
                ('_color.fp20', 'fp20'), ('_texture.fp20', 'fp20'), ('_tex_mod_color.fp20', 'fp20'),
                ('_tex_pass.fp20', 'fp20'), ('_diff.fp20', 'fp20')]:
    if os.path.exists(os.path.join(install, 'shaders', name)):
        add('..\\shaders\\' + name, 'main', p)


def run(j):
    fn, entry, p, dst = j
    src = open(os.path.join(bindir, fn.replace('\\', '/')), 'rb').read()
    if src.lstrip().startswith(b'!!'):
        open(dst, 'wb').write(src)
        return None
    r = subprocess.run([sys.executable, os.path.join(here, 'cgdll_emu.py'), os.path.join(bindir, 'cg.dll'),
                        p, entry, fn, dst], capture_output=True, text=True, cwd=bindir)
    if r.returncode or not os.path.exists(dst) or b'!!' not in open(dst, 'rb').read():
        return (fn, entry, (r.stdout + r.stderr)[-500:])
    return None


with cf.ThreadPoolExecutor(8) as ex:
    bad = [b for b in ex.map(run, jobs) if b]
json.dump(index, open(os.path.join(out, 'index.json'), 'w'), indent=1)
print(len(index), 'programs,', len(bad), 'failed')
for b in bad:
    print(b)
sys.exit(1 if bad else 0)
