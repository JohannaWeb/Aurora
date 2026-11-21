#!/usr/bin/env python3
"""Build Aurora's patched copies of vendored crates from crates.io sources.

Aurora carries changes to a few crates (blitz-dom, blitz-paint, stylo). They
are not checked in as forks; each crate has a patch series instead:

    scripts/patches/<crate>-<version>/0001-<subject>.patch, 0002-..., ...

`apply` downloads the pristine crate from crates.io (checking its sha256
against the crates.io index), commits it to a scratch git repository as the
`vendor-base` tag, and applies the series with `git am`. The result lands in
target/vendor-patched/<crate>-<version>/, which `[patch.crates-io]` in
Cargo.toml points at. Run it before the first `cargo build` and after a
series changes; when the series is unchanged it does nothing.

    scripts/vendor_patches.py apply [<crate> ...]
    scripts/vendor_patches.py regen [<crate> ...]
    scripts/vendor_patches.py status

To change a patch, edit the tree in target/vendor-patched/<crate>-<version>/,
commit there (one commit per patch; `git rebase -i vendor-base` to amend or
reorder), then `regen` to rewrite the series from vendor-base..HEAD. `regen`
rebuilds a scratch tree from the new series and refuses to write a series that
does not reproduce the working tree. See scripts/patches/README.md.
"""

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATCHES = ROOT / "scripts" / "patches"
OUT = ROOT / "target" / "vendor-patched"
DOWNLOADS = ROOT / "target" / "vendor-sources"
USER_AGENT = "aurora-vendor-patches (https://github.com/JohannaWeb/Aurora)"

# Written next to the materialized tree, excluded from its git status.
STAMP = ".aurora-vendor-stamp"

# Fixed identity and dates, so materialized trees and regenerated patches do
# not depend on the local git configuration.
GIT_ENV = {
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_CONFIG_SYSTEM": os.devnull,
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_AUTHOR_NAME": "aurora-vendor",
    "GIT_AUTHOR_EMAIL": "aurora-vendor@localhost",
    "GIT_COMMITTER_NAME": "aurora-vendor",
    "GIT_COMMITTER_EMAIL": "aurora-vendor@localhost",
    "GIT_AUTHOR_DATE": "2000-01-01T00:00:00 +0000",
    "GIT_COMMITTER_DATE": "2000-01-01T00:00:00 +0000",
}


class Crate:
    def __init__(self, series: Path):
        match = re.fullmatch(r"(.+?)-(\d+\.\d+\.\d+(?:[-+].+)?)", series.name)
        if not match:
            raise SystemExit(f"{series}: expected a <crate>-<version> directory name")
        self.series = series
        self.name, self.version = match.groups()
        self.tree = OUT / series.name

    def patches(self):
        return sorted(self.series.glob("*.patch"))

    def series_hash(self) -> str:
        digest = hashlib.sha256()
        for patch in self.patches():
            digest.update(patch.name.encode() + b"\0" + patch.read_bytes() + b"\0")
        return digest.hexdigest()[:16]


def crates(names=None):
    found = [Crate(d) for d in sorted(PATCHES.iterdir()) if d.is_dir()]
    if not names:
        return found
    by_name = {c.name: c for c in found}
    missing = [n for n in names if n not in by_name]
    if missing:
        raise SystemExit(f"no patch series for: {', '.join(missing)}")
    return [by_name[n] for n in names]


def git(tree: Path, *args, capture=False, check=True, input=None):
    env = dict(os.environ, **GIT_ENV)
    result = subprocess.run(
        ["git", "-C", str(tree), *args],
        env=env,
        check=check,
        text=True,
        input=input,
        capture_output=capture,
    )
    return result.stdout.strip() if capture else result


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req) as resp:
        return resp.read()


def index_path(name: str) -> str:
    """The crate's file in the crates.io sparse index."""
    lower = name.lower()
    if len(lower) <= 2:
        return f"{len(lower)}/{lower}"
    if len(lower) == 3:
        return f"3/{lower[0]}/{lower}"
    return f"{lower[:2]}/{lower[2:4]}/{lower}"


def expected_checksum(crate: Crate) -> str:
    lines = fetch(f"https://index.crates.io/{index_path(crate.name)}").decode().splitlines()
    for line in lines:
        entry = json.loads(line)
        if entry["vers"] == crate.version:
            return entry["cksum"]
    raise SystemExit(f"{crate.name} {crate.version} is not in the crates.io index")


def crate_archive(crate: Crate) -> bytes:
    """The published .crate, downloaded once and verified against the index."""
    cached = DOWNLOADS / f"{crate.name}-{crate.version}.crate"
    if cached.exists():
        return cached.read_bytes()
    data = fetch(f"https://static.crates.io/crates/{crate.name}/{crate.name}-{crate.version}.crate")
    actual = hashlib.sha256(data).hexdigest()
    expected = expected_checksum(crate)
    if actual != expected:
        raise SystemExit(f"{crate.name} {crate.version}: sha256 {actual}, index says {expected}")
    DOWNLOADS.mkdir(parents=True, exist_ok=True)
    cached.write_bytes(data)
    return data


def extract(archive: bytes, crate: Crate, dest: Path):
    prefix = f"{crate.name}-{crate.version}/"
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
        for member in tar.getmembers():
            if not member.name.startswith(prefix) or not (member.isfile() or member.isdir()):
                continue
            relative = member.name[len(prefix):]
            if not relative:
                continue
            target = dest / relative
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(tar.extractfile(member).read())
            target.chmod(0o644 | (member.mode & 0o111))


def read_stamp(tree: Path):
    try:
        return json.loads((tree / STAMP).read_text())
    except (OSError, ValueError):
        return None


def local_work(tree: Path, stamp) -> str:
    """Why `tree` holds work that rebuilding would destroy, or ''."""
    if not (tree / ".git").exists():
        return ""
    if git(tree, "status", "--porcelain", capture=True):
        return "uncommitted changes"
    if stamp and git(tree, "rev-parse", "HEAD", capture=True) != stamp.get("head"):
        return "commits not yet written to the series (run regen)"
    return ""


def materialize(crate: Crate, dest: Path, archive: bytes):
    """Build a fresh tree at `dest`: pristine source tagged vendor-base, then the series."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    extract(archive, crate, dest)
    git(dest, "init", "-q", "-b", "main")
    (dest / ".git" / "info" / "exclude").write_text(STAMP + "\n")
    git(dest, "add", "-A")
    git(dest, "commit", "-q", "-m", f"pristine {crate.name} {crate.version} (crates.io)")
    git(dest, "tag", "vendor-base")
    patches = crate.patches()
    if patches:
        result = git(dest, "am", "-q", "--whitespace=nowarn", "--committer-date-is-author-date",
                     *map(str, patches), check=False)
        if result.returncode != 0:
            raise SystemExit(f"{crate.name}: series does not apply; resolve in {dest} "
                             "(git am --continue), then run regen")


def apply(crate: Crate, force: bool) -> bool:
    stamp = read_stamp(crate.tree)
    wanted = crate.series_hash()
    if stamp and stamp.get("series") == wanted and crate.tree.exists():
        return False
    reason = local_work(crate.tree, stamp)
    if reason and not force:
        raise SystemExit(f"{crate.tree} has {reason}; refusing to rebuild it (--force discards)")
    materialize(crate, crate.tree, crate_archive(crate))
    write_stamp(crate, wanted)
    return True


def write_stamp(crate: Crate, series: str):
    head = git(crate.tree, "rev-parse", "HEAD", capture=True)
    (crate.tree / STAMP).write_text(json.dumps({"series": series, "head": head}) + "\n")


def tree_diff(a: Path, b: Path) -> str:
    result = subprocess.run(
        ["diff", "-r", "-q", "-x", ".git", "-x", STAMP, str(a), str(b)],
        text=True,
        capture_output=True,
    )
    return result.stdout


def regen(crate: Crate):
    if not (crate.tree / ".git").exists():
        raise SystemExit(f"{crate.tree} is not materialized; run apply first")
    if git(crate.tree, "status", "--porcelain", capture=True):
        raise SystemExit(f"{crate.tree} has uncommitted changes; commit them first")
    with tempfile.TemporaryDirectory() as scratch:
        new_series = Path(scratch) / "series"
        git(crate.tree, "format-patch", "-q", "--zero-commit", "--no-signature", "--no-numbered",
            "--no-stat", "-o", str(new_series), "vendor-base..HEAD")
        for patch in new_series.glob("*.patch"):
            text = "\n".join(line.rstrip() for line in patch.read_text().split("\n"))
            patch.write_text(text)
        check = Crate(new_series.parent / crate.series.name)
        check.series = new_series
        rebuilt = Path(scratch) / "rebuilt"
        materialize(check, rebuilt, crate_archive(crate))
        mismatch = tree_diff(rebuilt, crate.tree)
        if mismatch:
            raise SystemExit(f"{crate.name}: the regenerated series does not reproduce "
                             f"{crate.tree}; series left unchanged:\n{mismatch}")
        for old in crate.patches():
            old.unlink()
        crate.series.mkdir(parents=True, exist_ok=True)
        for patch in sorted(new_series.glob("*.patch")):
            shutil.copy(patch, crate.series / patch.name)
    write_stamp(crate, crate.series_hash())
    print(f"{crate.name}: {len(crate.patches())} patches in {crate.series.relative_to(ROOT)}")


def newest_version(name: str) -> str:
    data = json.loads(fetch(f"https://crates.io/api/v1/crates/{name}"))
    return data["crate"]["max_version"]


def status(items):
    for crate in items:
        stamp = read_stamp(crate.tree)
        if not crate.tree.exists():
            state = "not materialized"
        elif not stamp or stamp.get("series") != crate.series_hash():
            state = "stale (run apply)"
        else:
            state = local_work(crate.tree, stamp) or "up to date"
        try:
            newest = newest_version(crate.name)
        except OSError:
            newest = "?"
        note = f", newest on crates.io: {newest}" if newest != crate.version else ""
        print(f"{crate.name} {crate.version}: {len(crate.patches())} patches, {state}{note}")
        for patch in crate.patches():
            print(f"    {patch.name}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("apply", "regen", "status"):
        cmd = sub.add_parser(name)
        cmd.add_argument("crates", nargs="*", help="crate names (default: all)")
        if name == "apply":
            cmd.add_argument("--force", action="store_true",
                             help="rebuild even if the tree has local work")
    args = parser.parse_args(argv)
    items = crates(args.crates)
    if args.command == "apply":
        for crate in items:
            if apply(crate, args.force):
                print(f"{crate.name} {crate.version}: applied {len(crate.patches())} patches")
    elif args.command == "regen":
        for crate in items:
            regen(crate)
    else:
        status(items)


if __name__ == "__main__":
    main(sys.argv[1:])
