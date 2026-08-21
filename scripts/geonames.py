#!/usr/bin/env python3
"""Download, verify, and stage the pinned Oracle Studio GeoNames inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK = ROOT / "catalog" / "geonames.lock"
DEFAULT_SOURCE = ROOT / "var" / "geonames" / "source"
DEFAULT_CANDIDATE_SOURCE = ROOT / "var" / "geonames" / "candidate" / "source"
DEFAULT_CANDIDATE_LOCK = ROOT / "var" / "geonames" / "geonames.candidate.lock"
DEFAULT_OUTPUT = ROOT / "crates" / "oracle-studio-ui" / "dist"
DEFAULT_BASE_URL = "https://download.geonames.org/export/dump/"
NAMES = ("cities500.zip", "admin1CodesASCII.txt", "admin2Codes.txt")
LOCK_HEADER = re.compile(
    r"# GeoNames build input lock captured (\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z)\."
)
SHA256 = re.compile(r"[0-9a-f]{64}")
ATTRIBUTION = "Contains GeoNames geographical data, available under CC BY 4.0."
LICENSE_URL = "https://creativecommons.org/licenses/by/4.0/"


class GeoNamesError(RuntimeError):
    """A safe, user-facing GeoNames workflow failure."""


@dataclass(frozen=True)
class LockEntry:
    name: str
    sha256: str
    byte_length: int


@dataclass(frozen=True)
class Lock:
    captured_at: str
    entries: tuple[LockEntry, ...]


def read_lock(path: Path) -> Lock:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise GeoNamesError(f"cannot read GeoNames lock {path}: {error}") from error
    if not lines:
        raise GeoNamesError("GeoNames lock is empty")
    header = LOCK_HEADER.fullmatch(lines[0])
    if header is None:
        raise GeoNamesError("GeoNames lock has an invalid capture timestamp header")
    entries: list[LockEntry] = []
    for line in lines[1:]:
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 3:
            raise GeoNamesError(f"invalid GeoNames lock row: {line!r}")
        name, digest, length_text = fields
        if name not in NAMES or not SHA256.fullmatch(digest):
            raise GeoNamesError(f"invalid GeoNames lock identity: {line!r}")
        try:
            length = int(length_text)
        except ValueError as error:
            raise GeoNamesError(f"invalid GeoNames byte length: {length_text!r}") from error
        if length <= 0:
            raise GeoNamesError(f"invalid GeoNames byte length: {length}")
        entries.append(LockEntry(name, digest, length))
    if tuple(entry.name for entry in entries) != NAMES:
        raise GeoNamesError("GeoNames lock must contain the three required files in order")
    return Lock(header.group(1), tuple(entries))


def digest(path: Path) -> tuple[str, int]:
    sha256 = hashlib.sha256()
    length = 0
    try:
        with path.open("rb") as handle:
            for block in iter(lambda: handle.read(1024 * 1024), b""):
                sha256.update(block)
                length += len(block)
    except OSError as error:
        raise GeoNamesError(f"cannot read GeoNames input {path}: {error}") from error
    return sha256.hexdigest(), length


def verify(lock: Lock, source: Path) -> None:
    for entry in lock.entries:
        actual_sha256, actual_length = digest(source / entry.name)
        if actual_length != entry.byte_length or actual_sha256 != entry.sha256:
            raise GeoNamesError(
                f"GeoNames verification failed for {entry.name}: expected "
                f"{entry.byte_length} bytes/{entry.sha256}, got "
                f"{actual_length} bytes/{actual_sha256}"
            )


def fetch(url: str, destination: Path, timeout: int) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "oracle-studio-geonames/1"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response, destination.open(
            "wb"
        ) as output:
            shutil.copyfileobj(response, output, length=1024 * 1024)
    except (OSError, urllib.error.URLError) as error:
        raise GeoNamesError(f"GeoNames download failed for {url}: {error}") from error


def fetch_all(base_url: str, destination: Path, timeout: int) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    prefix = base_url.rstrip("/") + "/"
    for name in NAMES:
        fetch(prefix + name, destination / name, timeout)


def install_files(staged: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for name in NAMES:
        os.replace(staged / name, destination / name)


def download(lock_path: Path, source: Path, base_url: str, timeout: int) -> None:
    lock = read_lock(lock_path)
    source.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".geonames-download-", dir=source.parent) as raw:
        temporary = Path(raw)
        fetch_all(base_url, temporary, timeout)
        verify(lock, temporary)
        install_files(temporary, source)
    print(f"Verified GeoNames inputs installed in {source}")


def attribution_text(lock: Lock) -> str:
    return "\n".join(
        [
            ATTRIBUTION,
            "Source: https://download.geonames.org/export/dump/",
            f"License: CC BY 4.0 ({LICENSE_URL})",
            f"Pinned lock captured: {lock.captured_at}",
            "",
        ]
    )


def stage(lock_path: Path, source: Path, output: Path) -> None:
    lock = read_lock(lock_path)
    verify(lock, source)
    target = output / "catalog" / "geonames"
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".geonames-stage-", dir=target.parent))
    try:
        for entry in lock.entries:
            shutil.copyfile(source / entry.name, temporary / entry.name)
        manifest = {
            "retrieved_at": lock.captured_at,
            "cities500_sha256": lock.entries[0].sha256,
            "admin1_sha256": lock.entries[1].sha256,
            "admin2_sha256": lock.entries[2].sha256,
        }
        (temporary / "manifest.json").write_text(
            json.dumps(manifest, separators=(",", ":")) + "\n", encoding="utf-8"
        )
        (temporary / "ATTRIBUTION.txt").write_text(attribution_text(lock), encoding="utf-8")
        temporary.chmod(0o755)
        for path in temporary.iterdir():
            path.chmod(0o644)
        if target.exists():
            shutil.rmtree(target)
        os.replace(temporary, target)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    print(f"Verified GeoNames catalog staged in {target}")


def write_lock(path: Path, captured_at: str, entries: Sequence[LockEntry]) -> None:
    lines = [
        f"# GeoNames build input lock captured {captured_at}.",
        "# Candidate only; review deliberately before editing catalog/geonames.lock.",
        *(f"{entry.name} {entry.sha256} {entry.byte_length}" for entry in entries),
        "",
    ]
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text("\n".join(lines), encoding="utf-8")
    os.replace(temporary, path)


def candidate_lock(
    output: Path, source: Path, base_url: str, timeout: int, captured_at: str | None = None
) -> None:
    source.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".geonames-candidate-", dir=source.parent) as raw:
        temporary = Path(raw)
        fetch_all(base_url, temporary, timeout)
        entries = tuple(
            LockEntry(name, *digest(temporary / name))
            for name in NAMES
        )
        # digest() returns SHA-256 then byte length, matching LockEntry's fields.
        if any(entry.byte_length <= 0 for entry in entries):
            raise GeoNamesError("candidate GeoNames download contains an empty file")
        install_files(temporary, source)
    timestamp = captured_at or datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace(
        "+00:00", "Z"
    )
    write_lock(output, timestamp, entries)
    print(f"Candidate GeoNames lock written to {output}; tracked lock unchanged")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subcommands = result.add_subparsers(dest="command", required=True)

    check = subcommands.add_parser("check", help="verify cached inputs offline")
    check.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    check.add_argument("--source-dir", type=Path, default=DEFAULT_SOURCE)

    get = subcommands.add_parser("download", help="download only the tracked lock inputs")
    get.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    get.add_argument("--source-dir", type=Path, default=DEFAULT_SOURCE)
    get.add_argument("--base-url", default=DEFAULT_BASE_URL)
    get.add_argument("--timeout", type=int, default=120)

    build = subcommands.add_parser("stage", help="stage verified inputs into Trunk output")
    build.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    build.add_argument("--source-dir", type=Path, default=DEFAULT_SOURCE)
    build.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)

    candidate = subcommands.add_parser(
        "candidate-lock", help="download current upstream bytes into ignored review paths"
    )
    candidate.add_argument("--output", type=Path, default=DEFAULT_CANDIDATE_LOCK)
    candidate.add_argument("--source-dir", type=Path, default=DEFAULT_CANDIDATE_SOURCE)
    candidate.add_argument("--base-url", default=DEFAULT_BASE_URL)
    candidate.add_argument("--timeout", type=int, default=120)
    return result


def main(arguments: Sequence[str] | None = None) -> int:
    args = parser().parse_args(arguments)
    try:
        if args.command == "check":
            verify(read_lock(args.lock), args.source_dir)
            print(f"GeoNames inputs match {args.lock}")
        elif args.command == "download":
            download(args.lock, args.source_dir, args.base_url, args.timeout)
        elif args.command == "stage":
            stage(args.lock, args.source_dir, args.output)
        elif args.command == "candidate-lock":
            candidate_lock(args.output, args.source_dir, args.base_url, args.timeout)
        else:  # pragma: no cover - argparse enforces the command set.
            raise AssertionError(args.command)
    except GeoNamesError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
