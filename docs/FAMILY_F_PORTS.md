# Family F batch: Clear Sailing, Timbury, Mad Mod Mike, Ogre

Status: in progress (2026-11-27). Four parallel workers, one per demo, each on its own branch. Coordinator merges to main, then gallery + deploy verification.

## Source installers (never run)

| Demo | Installer | Size | SHA-1 (to confirm) |
|---|---|---|---|
| Clear Sailing | NVIDIA-demos/clear_sailing.exe | 119 MB | `shasum` at verify time |
| Timbury | NVIDIA-demos/timbury.exe | 119 MB | `shasum` at verify time |
| Mad Mod Mike | NVIDIA-demos/MadModMikeSetup.exe | 153 MB | `shasum` at verify time |
| Ogre | NVIDIA-demos/Ogre_1.1.exe | 238 MB | `shasum` at verify time |

## Unpack (already done once to $UNPACK)

```sh
cd gpu-demo-gallery
export UNPACK=/private/var/folders/bj/fw1cj85n1yz0_rwt9mb__6bh0000gn/T/unpack4
python3 tools/unpack/pftw.py /Users/archit/Dev/gpu-demos/NVIDIA-demos/clear_sailing.exe $UNPACK/cs
python3 tools/unpack/pftw.py /Users/archit/Dev/gpu-demos/NVIDIA-demos/timbury.exe $UNPACK/timbury
python3 tools/unpack/pftw.py /Users/archit/Dev/gpu-demos/NVIDIA-demos/MadModMikeSetup.exe $UNPACK/mmm
unar -f /Users/archit/Dev/gpu-demos/NVIDIA-demos/Ogre_1.1.exe  # then pftw-equivalent Disk1 layout at $UNPACK/ogre_nsis
python3 tools/unpack/iscab2.py $UNPACK/cs/Disk1 $UNPACK/cs/files
python3 tools/unpack/iscab2.py $UNPACK/timbury/Disk1 $UNPACK/timbury/files
python3 tools/unpack/iscab2.py $UNPACK/mmm/Disk1 $UNPACK/mmm/files
python3 tools/unpack/iscab2.py $UNPACK/ogre_nsis/Disk1 $UNPACK/ogre/files
```

Cabinets MD5-checked by iscab2 (cs: 530/534, timbury: 446/450, mmm: 860/864, ogre: 119/123).

## REA (each worker)

```sh
export JAVA_HOME=/opt/homebrew/opt/openjdk@25/libexec/openjdk.jdk/Contents/Home
export GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec
npx -y rea-agents@6.10.1 inspect --help
npx -y rea-agents@6.10.1 inspect <exe>
npx -y rea-agents@6.10.1 decompile-all <exe> --output <dir>
```

Workers: ses_de218126effeFJkVXy8qsScSi2 (clear-sailing), ses_de218049affeBM6HqOlZiiaB1q (timbury), ses_de217f57affeIWmiTokJ7Oq2N7 (mad-mod-mike), ses_de217e5faffeH12y2Xb7GJvjy4 (ogre).

## Targets

| Demo | Dir | Pages project |
|---|---|---|
| Clear Sailing | demos/clear-sailing | clear-sailing-port |
| Timbury | demos/timbury | timbury-port |
| Mad Mod Mike | demos/mad-mod-mike | mad-mod-mike-port |
| Ogre | demos/ogre | ogre-port |

Wrangler is authenticated (architrox796@gmail.com). Each demo deploys with its own scripts/deploy-cloudflare.sh; gallery deploys separately.

## Verify (for another agent)

1. `git log --oneline -8` shows the four port commits merged to main.
2. For each demo: `cd demos/<name> && scripts/dev.sh all` with data/ populated (see its DEVELOPMENT.md for data layout).
3. Check live URLs return 200: the four Pages sites + gallery.
4. Confirm original assets are not in git: `git ls-files demos/<name> | grep -i -E 'dds|tga|wav|mp3|nmb|nma'` is empty.
