#!/usr/bin/env python3
"""Contrôle de préservation des fonctionnalités (règle 7 de docs/agents/ORGANISATION.md).

Compare deux révisions git et liste ce qui a DISPARU :
  api        commandes de l'API interne (app/src/nf/api.rs)
  ui-api     commandes appelées par les pages d'interface (app/ui/*.js)
  reglage    champs des structures enregistrées (#[derive(... Serialize ...)])
  test       tests unitaires (#[test])
  autotest   étapes de l'auto-test de bout en bout (step("...") dans selftest.rs)
  asset      fichiers d'interface servis (app/src/nf/assets.rs)
  ui-id      identifiants d'éléments des pages (id="..." dans app/ui/*.html)
  ui-reglage réglages liés dans la page Paramètres (S.xxx, P.xxx dans settings.js)
  raccourci  raccourcis clavier documentés (lignes « raccourci: » de FONCTIONNALITES.md)
  fonction   lignes du registre docs/agents/FONCTIONNALITES.md (identifiants F-xxx)
  ci-job     jobs de la CI (.github/workflows/ci.yml)

Une disparition n'est acceptée que si elle est déclarée, une par ligne, dans un fichier
--allow ou dans la variable d'environnement nommée par --allow-env (description de PR) :
  Préservation-OK: <catégorie>:<élément> — <raison : renommé en X / suppression approuvée par le propriétaire le JJ/MM>

usage : preserve.py <base> <head> [--allow fichier] [--allow-env PR_BODY] [--list]
Code de sortie 1 s'il reste des disparitions non déclarées.
"""
import os
import re
import subprocess
import sys


def git(*args):
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} : {r.stderr.strip()}")
    return r.stdout


def files_at(rev):
    return git("ls-tree", "-r", "--name-only", rev).splitlines()


def read(rev, path):
    r = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True)
    return r.stdout.decode("utf-8", "replace") if r.returncode == 0 else ""


def strip_line_comments(src):
    return re.sub(r"(?m)^\s*//.*$", "", src)


def serialize_fields(src, path):
    out = set()
    for m in re.finditer(r"#\[derive\(([^)]*)\)\]\s*(?:#\[[^\]]*\]\s*)*pub\s+struct\s+(\w+)\s*\{", src):
        if "Serialize" not in m.group(1):
            continue
        depth, i, start = 1, m.end(), m.end()
        while i < len(src) and depth:
            depth += {"{": 1, "}": -1}.get(src[i], 0)
            i += 1
        body = src[start:i - 1]
        for f in re.finditer(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:", body):
            out.add(f"{m.group(2)}.{f.group(1)}")
    return out


def inventory(rev):
    inv = {k: set() for k in ("api", "ui-api", "reglage", "test", "autotest", "asset", "ui-id", "ui-reglage", "raccourci", "fonction", "ci-job")}
    files = files_at(rev)
    for p in files:
        if p.endswith(".rs") and (p.startswith("app/") or p.startswith("crates/") or p.startswith("xtask/")):
            src = strip_line_comments(read(rev, p))
            for m in re.finditer(r"#\[(?:tokio::)?test\][^\n]*\n(?:\s*#\[[^\]]*\]\s*\n)*\s*(?:async\s+)?fn\s+(\w+)", src):
                inv["test"].add(f"{p}::{m.group(1)}")
            if "Serialize" in src:
                inv["reglage"] |= {f"{p}:{x}" for x in serialize_fields(src, p)}
        if p == "app/src/nf/api.rs":
            src = strip_line_comments(read(rev, p))
            for line in src.splitlines():
                if "=>" in line or re.search(r'"\s*\|\s*$', line) or re.search(r'^\s*\|?\s*"[a-z]', line):
                    for m in re.finditer(r'"([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+)"', line):
                        inv["api"].add(m.group(1))
        if p == "app/src/nf/selftest.rs":
            for m in re.finditer(r'\bstep\("([^"]+)"\)', read(rev, p)):
                inv["autotest"].add(m.group(1))
        if p == "app/src/nf/assets.rs":
            for m in re.finditer(r'asset!\("([^"]+)"', read(rev, p)):
                inv["asset"].add(m.group(1))
        if p.startswith("app/ui/") and p.endswith(".js"):
            src = read(rev, p)
            for m in re.finditer(r"""\bapi\(\s*['"]([a-z][a-z0-9_.]+)['"]""", src):
                inv["ui-api"].add(m.group(1))
            if p.endswith("settings.js"):
                for m in re.finditer(r"\b([SP])\.([a-z_][a-z0-9_]*)\b", src):
                    inv["ui-reglage"].add(f"{m.group(1)}.{m.group(2)}")
        if p.startswith("app/ui/") and p.endswith(".html"):
            for m in re.finditer(r'\bid="([^"]+)"', read(rev, p)):
                inv["ui-id"].add(f"{os.path.basename(p)}#{m.group(1)}")
        if p == "docs/agents/FONCTIONNALITES.md":
            src = read(rev, p)
            for m in re.finditer(r"(?m)^\|\s*(F-\d{3,4})\s*\|", src):
                inv["fonction"].add(m.group(1))
            for m in re.finditer(r"(?m)raccourci:\s*`([^`]+)`", src):
                inv["raccourci"].add(m.group(1))
        if p == ".github/workflows/ci.yml":
            src = read(rev, p)
            jobs = src.split("\njobs:", 1)[1] if "\njobs:" in src else ""
            for m in re.finditer(r"(?m)^  ([A-Za-z0-9_-]+):\s*$", jobs):
                inv["ci-job"].add(m.group(1))
    return inv


def allowed_items(args):
    text = ""
    if "--allow" in args:
        text += open(args[args.index("--allow") + 1], encoding="utf-8").read()
    if "--allow-env" in args:
        text += "\n" + os.environ.get(args[args.index("--allow-env") + 1], "")
    items = set()
    for m in re.finditer(r"(?m)^\s*Préservation-OK:\s*([a-z-]+):(.+?)\s+—\s+\S", text):
        items.add(f"{m.group(1)}:{m.group(2).strip()}")
    return items


def main():
    args = sys.argv[1:]
    if len(args) < 2:
        print(__doc__)
        sys.exit(2)
    base, head = args[0], args[1]
    a, b = inventory(base), inventory(head)
    if "--list" in args:
        for k in b:
            print(f"{k}: {len(b[k])}")
        return
    ok = allowed_items(args)
    missing, declared = [], []
    for k in a:
        for item in sorted(a[k] - b[k]):
            (declared if f"{k}:{item}" in ok else missing).append(f"{k}:{item}")
    added = sum(len(b[k] - a[k]) for k in b)
    print(f"Préservation des fonctionnalités : {base} → {head}")
    print("  recensé : " + ", ".join(f"{k} {len(b[k])}" for k in b))
    print(f"  ajouts : {added}")
    for d in declared:
        print(f"  disparition déclarée : {d}")
    if missing:
        print(f"  DISPARITIONS NON DÉCLARÉES ({len(missing)}) :")
        for m in missing:
            print(f"    {m}")
        print("Chaque disparition doit être justifiée par une ligne « Préservation-OK: <catégorie>:<élément> — <raison> »")
        print("dans la description de la PR (renommage, ou suppression approuvée par le propriétaire).")
        sys.exit(1)
    print("  aucune disparition non déclarée")


if __name__ == "__main__":
    main()
