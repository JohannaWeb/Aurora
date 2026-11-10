#!/usr/bin/env python3
"""Rebase Aurora's vendored crate forks in third_party/ onto a new upstream release.

Each fork directory holds an UPSTREAM file naming the crates.io release it was
copied from, e.g. "blitz-dom 0.3.0-alpha.5". Updating does a three-way merge of
every file:

    base     = pristine copy of the release recorded in UPSTREAM
    ours     = third_party/<fork> (upstream + Aurora's patches)
    upstream = pristine copy of the new release

Files Aurora never touched take the new upstream version; files upstream never
touched keep Aurora's version; files both changed are merged with
`git merge-file`, leaving conflict markers where the edits overlap.

Usage:
    scripts/update_forks.py status
    scripts/update_forks.py update <fork> <new-version> [<fork> <new-version> ...]
    scripts/update_forks.py diff <fork>

`status` lists each fork's base, the newest release on crates.io, and which
files Aurora has patched. `diff` prints Aurora's patch against its base. After
an update, bump the matching versions in the root Cargo.toml, resolve any
conflicts the script lists, and update the fork's PATCHES.md.
"""

import filecmp
import io
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
THIRD_PARTY = ROOT / "third_party"
CACHE = ROOT / "target" / "fork-sources"
USER_AGENT = "aurora-update-forks (https://github.com/JohannaWeb/Aurora)"

# Files crates.io adds to every published crate, plus files Aurora adds to a
# fork. Neither take part in the merge.
PACKAGING_FILES = {".cargo_vcs_info.json", ".cargo-ok", "Cargo.toml.orig", "Cargo.lock"}
AURORA_FILES = {"UPSTREAM", "PATCHES.md"}


def forks():
    return sorted(p.parent for p in THIRD_PARTY.glob("*/UPSTREAM"))


def read_upstream(fork: Path):
    name, version = (fork / "UPSTREAM").read_text().split()
    return name, version


def fetch_json(url: str):
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


def pristine(name: str, version: str) -> Path:
    """Download and unpack a crates.io release once; return its directory."""
    dest = CACHE / f"{name}-{version}"
    if dest.is_dir():
        return dest
    url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req) as resp:
        data = resp.read()
    CACHE.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=CACHE) as tmp:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
            tar.extractall(tmp, filter="data")
        shutil.move(str(Path(tmp) / f"{name}-{version}"), dest)
    return dest


def files_under(directory: Path):
    return {
        p.relative_to(directory)
        for p in directory.rglob("*")
        if p.is_file() and p.name not in PACKAGING_FILES
    }


def same(a: Path, b: Path) -> bool:
    return a.is_file() and b.is_file() and filecmp.cmp(a, b, shallow=False)


def patched_files(fork: Path):
    name, version = read_upstream(fork)
    base = pristine(name, version)
    changed = []
    for rel in sorted(files_under(base) | files_under(fork)):
        if rel.name in AURORA_FILES and rel.parent == Path("."):
            continue
        b, o = base / rel, fork / rel
        if not b.exists():
            changed.append(("added", rel))
        elif not o.exists():
            changed.append(("deleted", rel))
        elif not same(b, o):
            changed.append(("modified", rel))
    return changed


def cmd_status():
    for fork in forks():
        name, version = read_upstream(fork)
        latest = fetch_json(f"https://crates.io/api/v1/crates/{name}")["crate"]["max_version"]
        marker = "" if latest == version else f"  (newer: {latest})"
        print(f"{fork.name}: {name} {version}{marker}")
        for kind, rel in patched_files(fork):
            print(f"    {kind:8} {rel}")


def cmd_diff(fork_name: str):
    fork = THIRD_PARTY / fork_name
    name, version = read_upstream(fork)
    base = pristine(name, version)
    for kind, rel in patched_files(fork):
        subprocess.run(
            ["git", "--no-pager", "diff", "--no-index", "--", str(base / rel), str(fork / rel)]
        )


def update(fork: Path, new_version: str):
    name, old_version = read_upstream(fork)
    if new_version == old_version:
        print(f"{fork.name}: already on {name} {new_version}")
        return []
    base = pristine(name, old_version)
    theirs = pristine(name, new_version)
    print(f"{fork.name}: {name} {old_version} -> {new_version}")

    conflicts = []
    for rel in sorted(files_under(base) | files_under(fork) | files_under(theirs)):
        if rel.name in AURORA_FILES and rel.parent == Path("."):
            continue
        b, o, t = base / rel, fork / rel, theirs / rel
        ours_untouched = (b.exists() and same(b, o)) or (not b.exists() and not o.exists())
        upstream_untouched = (b.exists() and same(b, t)) or (not b.exists() and not t.exists())

        if ours_untouched:
            # Aurora never changed this file: take upstream as-is.
            if t.exists():
                o.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(t, o)
            elif o.exists():
                o.unlink()
        elif upstream_untouched:
            pass  # Only Aurora changed it: keep ours.
        elif not b.exists():
            if same(o, t):
                continue
            # Both sides added the same path with different contents.
            conflicts.append((rel, "added by both; kept Aurora's copy, upstream's is in the cache"))
        elif not t.exists():
            conflicts.append((rel, "patched by Aurora but deleted upstream; kept Aurora's copy"))
        elif not o.exists():
            conflicts.append((rel, "deleted by Aurora but changed upstream; left deleted"))
        else:
            result = subprocess.run(
                [
                    "git", "merge-file",
                    "-L", f"aurora ({fork.name})",
                    "-L", f"{name} {old_version}",
                    "-L", f"{name} {new_version}",
                    str(o), str(b), str(t),
                ],
                capture_output=True,
                text=True,
            )
            if result.returncode < 0 or result.returncode > 127:
                sys.exit(f"git merge-file failed on {rel}: {result.stderr}")
            if result.returncode > 0:
                conflicts.append((rel, f"{result.returncode} conflict(s), markers left in file"))
            else:
                print(f"    merged   {rel}")

    # crates.io packaging files are not merged; a fork that carries them takes
    # the new release's copies so they never describe the old version.
    for packaging in PACKAGING_FILES:
        if (fork / packaging).exists() and (theirs / packaging).exists():
            shutil.copy2(theirs / packaging, fork / packaging)

    (fork / "UPSTREAM").write_text(f"{name} {new_version}\n")
    for rel, why in conflicts:
        print(f"    CONFLICT {rel}: {why}")
    return conflicts


def main(argv):
    if len(argv) < 2 or argv[1] in {"-h", "--help"}:
        print(__doc__)
        return 0
    cmd = argv[1]
    if cmd == "status":
        cmd_status()
    elif cmd == "diff" and len(argv) == 3:
        cmd_diff(argv[2])
    elif cmd == "update" and len(argv) >= 4 and len(argv) % 2 == 0:
        total = []
        for fork_name, version in zip(argv[2::2], argv[3::2]):
            fork = THIRD_PARTY / fork_name
            if not (fork / "UPSTREAM").is_file():
                sys.exit(f"{fork} has no UPSTREAM file")
            total += update(fork, version)
        if total:
            print(f"\n{len(total)} file(s) need manual resolution.")
            return 1
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
