"""Read raw native GPU fields or their lossless zstd archive."""

from pathlib import Path
import subprocess
import numpy as np


def read_native_fields(path, shape):
    path = Path(path)
    if path.exists():
        return np.memmap(path, mode="r", dtype="<f4").reshape(shape)
    data = subprocess.check_output(["zstd", "-d", "-q", "-c", str(path) + ".zst"])
    return np.frombuffer(data, dtype="<f4").reshape(shape)
