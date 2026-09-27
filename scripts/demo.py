#!/usr/bin/env python3

import os
import string
import shutil
from pathlib import Path
import subprocess

dirs = [
    # ".cache",
    ".coverage",
    ".DS_Store",
    ".mypy_cache",
    ".pylint_cache",
    ".pytest_cache",
    ".ruff_cache",
    "__pycache__",
]

files = [
    # file
    ".bash_history",
    ".python_history",
    "pip-log.txt",
]

file_endings = [
    # ".log",
    # ".o",
    ".pyc",
]

bad_examples = [
    ".logending",
    ".ofine",
    ".offline",
]

def create_large_file(name, size=30):
    with open(name,"wb") as f:
        f.seek((2**size)-1)
        f.write(b"\0")

def create_detritus(parent: str | Path):
    parent = Path(parent)

    for f in files:
        p = parent / f
        p.touch()

    for i in bad_examples:
        p = parent / i
        p.touch()

    for e in file_endings:
        for c in string.ascii_lowercase:
            p = parent / f'{c}{e}'
            p.touch()

    for d in dirs:
        p = parent / d
        os.mkdir(p)

    big_file = parent / "big.pyc"
    create_large_file(big_file)

    link_to_big_file = parent / "link_to_big_file.pyc"
    link_to_big_file.symlink_to(big_file.name)


def main():
    demo = Path('demo')
    if demo.exists():
        shutil.rmtree(demo)
        demo = Path('demo')
    demo.mkdir()
    create_detritus(demo)
    reclean = Path("target/debug/reclean")
    assert reclean.exists()
    subprocess.call(["../target/debug/reclean"], cwd=demo)

if __name__ == '__main__':
    main()
