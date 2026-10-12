"""Publish workspace packages in dependency order, allowing a partial retry."""
from pathlib import Path
import subprocess
import time
import tomllib
import urllib.error
import urllib.request


def published(name, version):
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": "we-breeze-json-release"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30):
            return True
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise


for manifest in ["macros/Cargo.toml", "Cargo.toml"]:
    package = tomllib.loads(Path(manifest).read_text())["package"]
    name, version = package["name"], package["version"]
    if published(name, version):
        print(f"Already published {name} {version}", flush=True)
        continue
    subprocess.run([
        "cargo", "publish", "--locked", "--registry", "crates-io",
        "--manifest-path", manifest,
    ], check=True)
    # Allow the API to catch up before a partially completed workflow is retried.
    for attempt in range(12):
        if published(name, version):
            break
        time.sleep(5)
    else:
        raise SystemExit(f"Cannot confirm publication of {name} {version}")
