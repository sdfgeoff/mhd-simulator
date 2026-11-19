"""Losslessly archive owned native fields, verifying decoded SHA before removing raw."""

import argparse, hashlib, json, subprocess
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("files", nargs="+", type=Path)
p.add_argument(
    "--log",
    type=Path,
    default=Path("artifacts/hardware-research/artifact-compression.jsonl"),
)
a = p.parse_args()
for path in a.files:
    archive = path.with_suffix(path.suffix + ".zst")
    if not path.exists():
        if archive.exists():
            print("Already archived:", path)
            continue
        raise FileNotFoundError(path)
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    if not archive.exists():
        subprocess.run(["zstd", "-q", "-3", str(path), "-o", str(archive)], check=True)
    process = subprocess.Popen(
        ["zstd", "-d", "-q", "-c", str(archive)], stdout=subprocess.PIPE
    )
    restored = hashlib.file_digest(process.stdout, "sha256").hexdigest()
    if process.wait() != 0 or restored != digest:
        raise ValueError("Archive verification failed: " + str(path))
    row = {
        "path": str(path),
        "original_bytes": path.stat().st_size,
        "archive_bytes": archive.stat().st_size,
        "sha256": digest,
        "decompression_sha256_verified": True,
    }
    with a.log.open("a") as out:
        out.write(json.dumps(row) + "\n")
    path.unlink()
    print(json.dumps(row), flush=True)
