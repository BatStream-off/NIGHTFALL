"""CI : manifeste signé du lecteur NIGHTFALL (player.json + player.json.sig).

    python3 manifest.py <version> <dossier>            écrit et signe
    python3 manifest.py --verify <dossier> <clé publique base64>

Signature Ed25519 sur  CONTEXT + octets de player.json. Le préfixe CONTEXT
sépare les deux usages de la clé de mise à jour : une signature du lecteur ne
vaut jamais pour un update.json de NIGHTFALL (signé sans préfixe), ni
l'inverse. La clé privée est lue dans NF_PLAYER_SIGNING_KEY (base64, 32
octets) et n'est jamais écrite ni affichée.
"""
import base64
import hashlib
import json
import os
import sys

CONTEXT = b"NIGHTFALL-LECTEUR-MANIFESTE-v1\n"
FILES = {
    "windows-x64": "Lecteur-NIGHTFALL-{v}-windows-x64.zip",
    "macos-arm64": "Lecteur-NIGHTFALL-{v}-macos-arm64.zip",
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write(version, out):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    assets = {}
    for plat, pattern in FILES.items():
        name = pattern.format(v=version)
        path = os.path.join(out, name)
        if not os.path.exists(path):
            sys.exit(f"absent : {name}")
        assets[plat] = {"file": name, "sha256": sha256(path), "size": os.path.getsize(path)}
    manifest = json.dumps({"kind": "nightfall-lecteur", "version": version, "ecs": os.environ.get("ECS_VERSION", ""),
                           "assets": assets}, sort_keys=True, separators=(",", ":")).encode()
    with open(os.path.join(out, "player.json"), "wb") as f:
        f.write(manifest)
    key = Ed25519PrivateKey.from_private_bytes(base64.b64decode(os.environ["NF_PLAYER_SIGNING_KEY"]))
    with open(os.path.join(out, "player.json.sig"), "w") as f:
        f.write(base64.b64encode(key.sign(CONTEXT + manifest)).decode())
    print(manifest.decode())


def verify(out, public_b64):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    manifest = open(os.path.join(out, "player.json"), "rb").read()
    sig = base64.b64decode(open(os.path.join(out, "player.json.sig")).read())
    pub = Ed25519PublicKey.from_public_bytes(base64.b64decode(public_b64))
    pub.verify(sig, CONTEXT + manifest)  # lève une exception si invalide
    try:
        pub.verify(sig, manifest)
        sys.exit("ÉCHEC : la signature vaut aussi sans le préfixe")
    except Exception:
        pass
    m = json.loads(manifest)
    for plat, a in m["assets"].items():
        path = os.path.join(out, a["file"])
        if os.path.getsize(path) != a["size"] or sha256(path) != a["sha256"]:
            sys.exit(f"ÉCHEC : {a['file']} ne correspond pas au manifeste")
    print(f"manifeste du lecteur {m['version']} : signature valide, {len(m['assets'])} paquets conformes, préfixe exigé")


if __name__ == "__main__":
    if sys.argv[1] == "--verify":
        verify(sys.argv[2], sys.argv[3])
    else:
        write(sys.argv[1], sys.argv[2])
