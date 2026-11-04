#!/usr/bin/env python3
"""Fiches de mission de l'organisation multi-agents NIGHTFALL.

  mission.py new <rôle> <slug> [--titre "Titre lisible"]   crée la fiche depuis le modèle
  mission.py list                                         missions, rôles, modèles, statuts
  mission.py status <ID> <STATUT>                         change le statut
  mission.py scope <ID> [--base origin/develop] [--ref <branche ou commit>]
                                                          vérifie que la branche ne touche
                                                          que les fichiers autorisés
  mission.py show <ID>                                    chemin de la fiche
  mission.py worktree <ID> [--from origin/develop]        crée la copie isolée de l'agent
                                                          (../wt-<ID>, branche de la fiche)

Statuts : PRÉPARÉE, EN COURS, LIVRÉE, EN AUDIT, ACCEPTÉE, REFUSÉE, INTÉGRÉE, ABANDONNÉE.
"""
import datetime
import fnmatch
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MISSIONS = os.path.join(ROOT, "docs", "agents", "missions")
TEMPLATE = os.path.join(ROOT, "docs", "agents", "templates", "MISSION.md")
ROLES = ("analyste", "architecte", "executeur", "optimiseur", "auditeur")
STATUSES = ("PRÉPARÉE", "EN COURS", "LIVRÉE", "EN AUDIT", "ACCEPTÉE", "REFUSÉE", "INTÉGRÉE", "ABANDONNÉE")
ALWAYS_ALLOWED = ("docs/agents/missions/{dir}/**",)


def die(msg):
    print(msg, file=sys.stderr)
    sys.exit(1)


def mission_dirs():
    if not os.path.isdir(MISSIONS):
        return []
    return sorted(d for d in os.listdir(MISSIONS) if re.match(r"M-\d{4}-", d))


def find(mid):
    mid = mid.upper() if mid.lower().startswith("m-") else f"M-{int(mid):04d}"
    for d in mission_dirs():
        if d.startswith(mid + "-") or d == mid:
            return d
    die(f"mission {mid} introuvable dans {MISSIONS}")


def field(text, name):
    m = re.search(rf"^- \*\*{re.escape(name)}\*\* : *(.*)$", text, re.M)
    return m.group(1).strip() if m else ""


def cmd_new(args):
    if len(args) < 2:
        die("usage : mission.py new <rôle> <slug> [--titre ...]")
    role, slug = args[0].lower(), args[1].lower()
    if role not in ROLES:
        die(f"rôle inconnu : {role} (attendu : {', '.join(ROLES)})")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,40}", slug):
        die("slug : minuscules, chiffres et tirets, 2 à 41 caractères")
    title = args[args.index("--titre") + 1] if "--titre" in args else slug.replace("-", " ").capitalize()
    nums = [int(d[2:6]) for d in mission_dirs()]
    mid = f"M-{(max(nums) + 1) if nums else 1:04d}"
    d = os.path.join(MISSIONS, f"{mid}-{slug}")
    os.makedirs(d)
    text = open(TEMPLATE, encoding="utf-8").read()
    for k, v in {"{ID}": mid, "{TITRE}": title, "{ROLE}": role.capitalize(), "{SLUG}": slug,
                 "{DATE}": datetime.date.today().isoformat()}.items():
        text = text.replace(k, v)
    path = os.path.join(d, "mission.md")
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    print(os.path.relpath(path, ROOT))


def cmd_list(_args):
    rows = []
    for d in mission_dirs():
        p = os.path.join(MISSIONS, d, "mission.md")
        t = open(p, encoding="utf-8").read() if os.path.exists(p) else ""
        rows.append((d, field(t, "Rôle"), field(t, "Modèle").split(" ")[0], field(t, "Statut"),
                     "rapport" if os.path.exists(os.path.join(MISSIONS, d, "rapport.md")) else "",
                     "audit" if os.path.exists(os.path.join(MISSIONS, d, "audit.md")) else ""))
    for r in rows:
        print(" | ".join(r))
    if not rows:
        print("(aucune mission)")


def cmd_status(args):
    if len(args) < 2:
        die("usage : mission.py status <ID> <STATUT>")
    st = " ".join(args[1:]).upper()
    if st not in STATUSES:
        die(f"statut inconnu : {st} (attendu : {', '.join(STATUSES)})")
    p = os.path.join(MISSIONS, find(args[0]), "mission.md")
    t = open(p, encoding="utf-8").read()
    t2 = re.sub(r"^(- \*\*Statut\*\* : *).*$", lambda m: m.group(1) + st, t, count=1, flags=re.M)
    open(p, "w", encoding="utf-8").write(t2)
    print(f"{find(args[0])} : {st}")


def glob_match(path, pattern):
    if pattern.endswith("/**"):
        return path == pattern[:-3] or path.startswith(pattern[:-2])
    if "**" in pattern:
        rx = re.escape(pattern).replace(r"\*\*/", "(?:.*/)?").replace(r"\*\*", ".*").replace(r"\*", "[^/]*").replace(r"\?", "[^/]")
        return re.fullmatch(rx, path) is not None
    return fnmatch.fnmatchcase(path, pattern) and path.count("/") == pattern.count("/")


def cmd_scope(args):
    if not args:
        die("usage : mission.py scope <ID> [--base origin/develop] [--ref <branche>]")
    d = find(args[0])
    t = open(os.path.join(MISSIONS, d, "mission.md"), encoding="utf-8").read()
    base = args[args.index("--base") + 1] if "--base" in args else "origin/develop"
    ref = args[args.index("--ref") + 1] if "--ref" in args else field(t, "Branche")
    if not ref:
        die("branche introuvable dans la fiche ; précise --ref")
    m = re.search(r"<!-- scope -->(.*?)<!-- /scope -->", t, re.S)
    patterns = [l.strip() for l in (m.group(1) if m else "").splitlines() if l.strip() and not l.strip().startswith("#")]
    patterns += [p.format(dir=d) for p in ALWAYS_ALLOWED]
    out = subprocess.run(["git", "-C", ROOT, "diff", "--name-only", f"{base}...{ref}"], capture_output=True, text=True)
    if out.returncode != 0:
        die(out.stderr.strip() or "git diff a échoué")
    changed = [l for l in out.stdout.splitlines() if l]
    outside = [c for c in changed if not any(glob_match(c, p) for p in patterns)]
    print(f"{d} : {len(changed)} fichier(s) modifié(s) entre {base} et {ref}")
    if outside:
        print("HORS PÉRIMÈTRE :")
        for c in outside:
            print(f"  {c}")
        sys.exit(1)
    print("périmètre respecté")


def cmd_worktree(args):
    if not args:
        die("usage : mission.py worktree <ID> [--from origin/develop]")
    d = find(args[0])
    t = open(os.path.join(MISSIONS, d, "mission.md"), encoding="utf-8").read()
    branch = field(t, "Branche")
    if not branch.startswith("agent/"):
        die("la fiche doit nommer une branche agent/...")
    start = args[args.index("--from") + 1] if "--from" in args else "origin/develop"
    path = os.path.join(os.path.dirname(ROOT), f"wt-{d[:6]}")
    if os.path.exists(path):
        die(f"{path} existe déjà")
    subprocess.run(["git", "-C", ROOT, "fetch", "-q", "origin"], check=True)
    r = subprocess.run(["git", "-C", ROOT, "worktree", "add", "-q", "-b", branch, path, start], capture_output=True, text=True)
    if r.returncode != 0:
        die(r.stderr.strip())
    print(path)


def self_test():
    cases = [
        ("app/ui/settings.js", "app/ui/**", True),
        ("app/ui/x/y.js", "app/ui/*.js", False),
        ("app/ui/settings.js", "app/ui/*.js", True),
        ("app/src/nf/windows.rs", "app/src/nf/windows.rs", True),
        ("app/src/nf/windows.rs", "app/**/*.rs", True),
        ("docs/agents/missions/M-0001-x/rapport.md", "docs/agents/missions/M-0001-x/**", True),
        ("docs/STATUS.md", "docs/agents/**", False),
    ]
    bad = [(p, g) for p, g, want in cases if glob_match(p, g) != want]
    for p, g in bad:
        print(f"ÉCHEC : {p} ~ {g}")
    print(f"mission.py : {len(cases) - len(bad)}/{len(cases)} cas conformes")
    return 1 if bad else 0


def main():
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__)
        return
    if sys.argv[1] == "--self-test":
        sys.exit(self_test())
    cmds = {"new": cmd_new, "list": cmd_list, "status": cmd_status, "scope": cmd_scope,
            "worktree": cmd_worktree, "show": lambda a: print(os.path.join("docs/agents/missions", find(a[0]), "mission.md"))}
    if sys.argv[1] not in cmds:
        die(f"commande inconnue : {sys.argv[1]}")
    cmds[sys.argv[1]](sys.argv[2:])


if __name__ == "__main__":
    main()
