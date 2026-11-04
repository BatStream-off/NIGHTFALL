"""CI: write out/update.json (installers' SHA-256 and size per platform) and,
when the NIGHTFALL_UPDATE_KEY secret is configured, out/update.json.sig
(Ed25519 signature, base64). Without the secret no signature is produced
and NIGHTFALL will not install the update automatically."""
import base64
import hashlib
import json
import os
import sys

tag, out = sys.argv[1], sys.argv[2]
version = tag.lstrip("v")
platforms = {
    "windows-x64": f"NIGHTFALL-{version}-windows-x64-setup.exe",
    "macos-arm64": f"NIGHTFALL-{version}-macos-arm64.dmg",
}
assets = {}
for key, name in platforms.items():
    path = os.path.join(out, name)
    if not os.path.exists(path):
        continue
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    assets[key] = {"file": name, "sha256": h.hexdigest(), "size": os.path.getsize(path)}
manifest = json.dumps({"version": version, "assets": assets}, sort_keys=True, separators=(",", ":")).encode()
with open(os.path.join(out, "update.json"), "wb") as f:
    f.write(manifest)
print(manifest.decode())

key_b64 = os.environ.get("NIGHTFALL_UPDATE_KEY", "").strip()
if not key_b64:
    print("NIGHTFALL_UPDATE_KEY absent : manifeste non signé (pas de mise à jour automatique pour cette version).")
    sys.exit(0)
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
key = Ed25519PrivateKey.from_private_bytes(base64.b64decode(key_b64))
sig = base64.b64encode(key.sign(manifest)).decode()
with open(os.path.join(out, "update.json.sig"), "w") as f:
    f.write(sig)
print("manifeste signé")
