#!/usr/bin/env python3
"""Garde-fou PreToolUse de l'organisation multi-agents NIGHTFALL.

Claude Code appelle ce script avant chaque appel d'outil (voir
.claude/settings.json et le champ `hooks` des fichiers .claude/agents/nf-*.md).
L'entrée est un JSON sur stdin ; `agent_id` n'est présent que lorsque l'appel vient
d'un sous-agent. Code de sortie 2 = appel refusé (le message sur stderr est montré à
l'agent). Code 0 = autorisé.

Règles (docs/agents/ORGANISATION.md, §6) :
- pour tous, Directeur compris : jamais de push forcé ni de suppression de `main` ou
  `develop` ;
- pour les agents : push seulement vers `agent/*`, pas de fusion, pas de sortie du
  mode brouillon, pas de version, de secret, d'étiquette ni de réglage du dépôt, pas
  de contournement des crochets git, pas de modification des garde-fous ;
- pour l'Auditeur, en plus : aucune écriture (ni commit, ni push, ni PR, ni requête
  d'API qui modifie quelque chose).

Le script n'a besoin que de la bibliothèque standard. `--self-test` vérifie les règles.
"""
import json
import os
import re
import shlex
import sys
import time

PROTECTED_BRANCHES = ("main", "develop")
LOG = os.path.join(os.path.expanduser("~"), ".claude", "nightfall", "guard.log")

# Fichiers que seuls le Directeur et le propriétaire peuvent modifier.
GUARD_PATHS = (
    "tools/agents/guard.py",
    "tools/agents/git-hooks",
    "tools/agents/install.sh",
    "tools/agents/preserve.py",
    "tools/agents/mission.py",
    ".claude/settings",
    ".claude/agents",
    ".claude/nightfall",
    ".git/hooks",
    ".git/config",
    "docs/agents/ORGANISATION.md",
    "docs/agents/JOURNAL.md",
    "docs/agents/templates",
)
# Commandes qui ne font que lire : autorisées même si elles citent un fichier protégé.
READ_ONLY = {"cat", "head", "tail", "less", "more", "grep", "rg", "wc", "ls", "diff",
             "file", "stat", "sha256sum", "python3 -I -m py_compile", "test", "["}
READ_ONLY_GIT = {"diff", "log", "show", "status", "blame", "ls-files", "ls-tree", "grep", "rev-parse"}

# Interdit à tout agent, où que ce soit dans la commande (y compris dans bash -c "...").
AGENT_FORBIDDEN = [
    (r"\bNF_DIRECTOR\b", "marqueur réservé au Directeur"),
    (r"\bgh\s+pr\s+(merge|ready)\b", "fusionner ou sortir une PR du brouillon est réservé au Directeur"),
    (r"\bgh\s+pr\s+edit\b[^\n;&|]*--base\b", "changer la branche cible d'une PR est réservé au Directeur"),
    (r"\bgh\s+(release|secret|variable|ruleset)\b", "versions, secrets et règles du dépôt sont réservés au Directeur"),
    (r"\bgh\s+repo\s+(edit|delete|rename|archive|sync)\b", "réglages du dépôt réservés au Directeur"),
    (r"\bgh\s+workflow\s+(enable|disable)\b", "activer ou désactiver un workflow est réservé au Directeur"),
    (r"\bgh\s+auth\b", "l'authentification GitHub ne se modifie pas"),
    (r"\b(curl|wget)\b[^\n]*api\.github\.com", "passe par gh, pas par curl, pour l'API GitHub"),
    (r"\bgit\b[^\n;&|]*\s--no-verify\b", "contourner les crochets git est interdit"),
    (r"\bgit\s+commit\b[^\n;&|]*\s-n\b", "contourner les crochets git est interdit"),
    (r"\bcore\.hooksPath\b", "les crochets git ne se modifient pas"),
    (r"\bgit\s+(tag|update-ref)\b(?!\s+(-l|--list)\b)", "étiquettes et références réservées au Directeur"),
    (r"\bgit\s+branch\b[^\n;&|]*\s-(D|d|M|m|f)\b", "renommer, forcer ou supprimer une branche est réservé au Directeur"),
    (r"NIGHTFALL_UPDATE_KEY|upd_priv", "la clé de signature des mises à jour ne se manipule jamais"),
]
AUDITOR_FORBIDDEN = [
    (r"\bgit\s+(commit|push|merge|rebase|cherry-pick|revert|stash\s+pop|am|apply)\b", "l'Auditeur ne modifie rien"),
    (r"\bgh\s+pr\s+(create|comment|review|close|reopen|edit)\b", "l'Auditeur ne modifie rien"),
    (r"\bgh\s+issue\s+(create|comment|close|edit)\b", "l'Auditeur ne modifie rien"),
    (r"\bgh\s+run\s+(cancel|rerun|delete)\b", "l'Auditeur ne modifie rien"),
]

# Points d'API GitHub : toujours interdits aux agents, ou interdits en écriture.
API_ALWAYS = re.compile(r"(/merges?$|/merges?/|ready_for_review|auto_merge|convert_to_draft|/update-branch)")
API_NO_WRITE = re.compile(r"/(releases|secrets|variables|rulesets|collaborators|keys|hooks|protection|environments|branches|tags|actions/permissions)(/|$)|/git/(refs|tags)(/|$)|^/?repos/[^/]+/[^/]+/?$")


def gh_api_call(args):
    """(méthode, chemin, champs) d'un appel `gh api`."""
    method, path, fields = None, None, []
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("-X", "--method") and i + 1 < len(args):
            method = args[i + 1].upper()
            i += 2
            continue
        if a.startswith("-X") and len(a) > 2:
            method = a[2:].upper()
        elif a.startswith("--method="):
            method = a.split("=", 1)[1].upper()
        elif a in ("-f", "-F", "--field", "--raw-field", "--input") and i + 1 < len(args):
            fields.append(args[i + 1])
            i += 2
            continue
        elif a in ("-H", "--header", "-q", "--jq", "-t", "--template", "--hostname", "--cache", "-p", "--preview") and i + 1 < len(args):
            i += 2
            continue
        elif not a.startswith("-") and path is None:
            path = a
        i += 1
    if method is None:
        method = "POST" if fields else "GET"
    return method, (path or "").split("?", 1)[0], fields


def check_gh_api(args, agent_type):
    method, path, fields = gh_api_call(args)
    if API_ALWAYS.search(path) and method != "GET":
        return "fusion et changement d'état de PR réservés au Directeur"
    if method != "GET" and API_NO_WRITE.search(path):
        return "versions, branches, secrets et réglages du dépôt réservés au Directeur"
    if method == "PATCH" and re.search(r"/pulls/\d+$", path):
        names = {f.split("=", 1)[0] for f in fields}
        if not names <= {"title", "body"}:
            return "un agent ne modifie que le titre et la description de sa PR"
    if agent_type == "nf-auditeur" and method != "GET":
        return "l'Auditeur ne fait que des lectures d'API"
    return None


def gh_sub(ws):
    i = 0
    while i < len(ws) and "=" in ws[i] and not ws[i].startswith("-"):
        i += 1
    if i < len(ws) and os.path.basename(ws[i]) == "gh":
        return ws[i + 1:]
    return None


SPLIT = re.compile(r"&&|\|\||;|\||\n|\$\(|`")


def segments(command):
    return [s.strip() for s in SPLIT.split(command) if s.strip()]


def words(segment):
    try:
        return shlex.split(segment)
    except ValueError:
        return segment.split()


def git_subcommand(ws):
    """('push', [args]) pour une commande git, en sautant -C/-c et leurs valeurs."""
    if not ws:
        return None, []
    i = 0
    while i < len(ws) and "=" in ws[i] and not ws[i].startswith("-"):
        i += 1  # variables d'environnement VAR=val
    if i < len(ws) and os.path.basename(ws[i]) in ("env", "command", "exec", "nohup", "time"):
        i += 1
        while i < len(ws) and "=" in ws[i]:
            i += 1
    if i >= len(ws) or os.path.basename(ws[i]) != "git":
        return None, []
    i += 1
    while i < len(ws) and ws[i].startswith("-"):
        if ws[i] in ("-C", "-c", "--git-dir", "--work-tree"):
            i += 2
        else:
            i += 1
    if i >= len(ws):
        return None, []
    return ws[i], ws[i + 1:]


def ref_target(spec):
    """Branche distante visée par un refspec de push."""
    if spec.startswith("+"):
        return None, True
    dst = spec.split(":", 1)[1] if ":" in spec else spec
    dst = dst.removeprefix("refs/heads/")
    return dst, False


def check_push(args, is_agent):
    forced = any(a in ("-f", "--force", "--force-with-lease", "--force-if-includes", "--mirror", "--delete", "-d", "--prune", "--all", "--tags")
                 or a.startswith("--force-with-lease=") for a in args)
    positional = [a for a in args if not a.startswith("-")]
    specs = positional[1:]  # positional[0] = dépôt distant
    for s in specs:
        dst, plus = ref_target(s)
        if s.startswith(":"):
            dst = s[1:].removeprefix("refs/heads/")
            forced = True
        if dst in PROTECTED_BRANCHES and (forced or plus):
            return "push forcé ou suppression de main/develop interdit"
        if is_agent:
            if plus or forced:
                return "un agent ne force jamais un push et ne supprime aucune branche"
            if not dst or not dst.startswith("agent/"):
                return f"un agent ne pousse que vers une branche agent/* (cible : {dst or '?'})"
    if forced and not specs:
        return "push forcé sans cible explicite interdit"
    if is_agent and forced:
        return "un agent ne force jamais un push"
    if is_agent and not specs:
        return "un agent doit nommer sa branche : git push -u origin agent/<mission>"
    return None


def runs_check_tool(ws):
    """Exécuter un outil de contrôle (sans le modifier) est permis aux agents."""
    if len(ws) < 2 or os.path.basename(ws[0]) not in ("python3", "python"):
        return False
    args = [w for w in ws[1:] if w != "-I"]
    if not args:
        return False
    script, rest = args[0], args[1:]
    name = os.path.basename(script)
    if name == "preserve.py":
        return True
    if name == "guard.py":
        return rest == ["--self-test"]
    if name == "mission.py":
        return bool(rest) and rest[0] in ("scope", "list", "show", "--self-test", "--help")
    return False


def touches_guard_path(text):
    return any(p in text for p in GUARD_PATHS)


def check_bash(command, agent_type, is_agent):
    for seg in segments(command):
        ws = words(seg)
        sub, args = git_subcommand(ws)
        if sub == "push":
            r = check_push(args, is_agent)
            if r:
                return r
    if not is_agent:
        return None
    for pat, why in AGENT_FORBIDDEN:
        if re.search(pat, command):
            return why
    for seg in segments(command):
        g = gh_sub(words(seg))
        if g and g[0] == "api":
            r = check_gh_api(g[1:], agent_type)
            if r:
                return r
    # Commandes imbriquées (bash -c "...", sh -c '...') : vérifiées aussi.
    for m in re.finditer(r"\b(?:ba|z)?sh\s+-c\s+(\"[^\"]*\"|'[^']*')", command):
        inner = m.group(1)[1:-1]
        r = check_bash(inner, agent_type, is_agent)
        if r:
            return r
    # Fusion, rebase ou reset d'une branche protégée dans la copie de travail de l'agent.
    for seg in segments(command):
        sub, args = git_subcommand(words(seg))
        if sub in ("checkout", "switch") and any(a in PROTECTED_BRANCHES for a in args) and not any(a in ("-b", "-B", "-c", "-C", "--detach") for a in args):
            return "un agent reste sur sa branche agent/* (pas de checkout de main/develop ; utilise origin/develop en lecture)"
    if agent_type == "nf-auditeur":
        for pat, why in AUDITOR_FORBIDDEN:
            if re.search(pat, command):
                return why
    if touches_guard_path(command):
        for seg in segments(command):
            if not touches_guard_path(seg):
                continue
            ws = words(seg)
            sub, _ = git_subcommand(ws)
            first = os.path.basename(ws[0]) if ws else ""
            if sub in READ_ONLY_GIT or (sub is None and first in READ_ONLY and ">" not in seg):
                continue
            if runs_check_tool(ws) and ">" not in seg:
                continue
            return "les garde-fous et la charte ne sont modifiables que par le Directeur"
    return None


def check_file_tool(tool_input, is_agent):
    if not is_agent:
        return None
    path = str(tool_input.get("file_path") or tool_input.get("notebook_path") or "")
    home_claude = os.path.join(os.path.expanduser("~"), ".claude")
    if touches_guard_path(path) or path.startswith(home_claude):
        return "les garde-fous et la charte ne sont modifiables que par le Directeur"
    return None


def decide(data):
    tool = data.get("tool_name", "")
    tool_input = data.get("tool_input") or {}
    agent_id = data.get("agent_id")
    agent_type = data.get("agent_type") or ""
    is_agent = bool(agent_id)
    if tool == "Bash":
        return check_bash(str(tool_input.get("command", "")), agent_type, is_agent)
    if tool in ("Edit", "Write", "NotebookEdit", "MultiEdit"):
        return check_file_tool(tool_input, is_agent)
    if tool == "Agent" and is_agent:
        return "un agent ne lance pas d'autre agent : c'est le rôle du Directeur"
    return None


def log(data, reason):
    try:
        os.makedirs(os.path.dirname(LOG), exist_ok=True)
        with open(LOG, "a", encoding="utf-8") as f:
            entry = {
                "t": time.strftime("%Y-%m-%dT%H:%M:%S"),
                "agent_id": data.get("agent_id"),
                "agent_type": data.get("agent_type"),
                "tool": data.get("tool_name"),
                "input": str((data.get("tool_input") or {}).get("command") or (data.get("tool_input") or {}).get("file_path") or "")[:300],
                "refus": reason,
            }
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
    except OSError:
        pass


def self_test():
    A = {"agent_id": "a1", "agent_type": "nf-executeur"}
    AUD = {"agent_id": "a2", "agent_type": "nf-auditeur"}
    D = {}

    def bash(ctx, cmd):
        return decide(dict(ctx, tool_name="Bash", tool_input={"command": cmd}))

    cases = [
        (A, "git push -u origin agent/M-0001-inventaire", False),
        (A, "cd /x && git push origin HEAD:agent/M-0002-x", False),
        (A, "git push origin develop", True),
        (A, "git push origin HEAD:refs/heads/main", True),
        (A, "git push", True),
        (A, "git push -f origin agent/M-0001-x", True),
        (A, "git push origin +agent/M-0001-x", True),
        (A, "bash -c 'gh pr merge 12'", True),
        (A, "gh api -X PUT repos/o/r/pulls/3/merge", True),
        (A, "gh api repos/o/r/pulls/3/ccr/ready_for_review -X POST", True),
        (A, "gh api repos/o/r/pulls -X POST -f head=agent/M-1 -f base=develop -F draft=true", False),
        (A, "gh run view 123 --json jobs", False),
        (A, "gh release create v9", True),
        (A, "git commit --no-verify -m x", True),
        (A, "git -c core.hooksPath=/dev/null commit -m x", True),
        (A, "NF_DIRECTOR=1 git push origin develop", True),
        (A, "git checkout develop", True),
        (A, "git checkout -b agent/M-3-x origin/develop", False),
        (A, "git diff origin/develop...HEAD", False),
        (A, "cat tools/agents/guard.py", False),
        (A, "sed -i s/a/b/ tools/agents/guard.py", True),
        (A, "echo x > .claude/settings.json", True),
        (A, "git tag v1", True),
        (A, "cargo test -p nf-shield", False),
        (A, "cat crates/nf-crypto/src/keys.rs", False),
        (A, "gh api repos/o/r/releases --jq '.[0].tag_name'", False),
        (A, "gh api repos/o/r/git/refs -f ref=refs/heads/develop -f sha=abc", True),
        (A, "gh api -X PATCH repos/o/r/pulls/7 -f body=rapport", False),
        (A, "gh api -X PATCH repos/o/r/pulls/7 -f base=main", True),
        (A, "gh api -X DELETE repos/o/r/git/refs/heads/develop", True),
        (A, "sh -c \"git push origin develop\"", True),
        (A, "curl -X PUT https://api.github.com/repos/o/r/pulls/1/merge", True),
        (AUD, "git log --oneline -5", False),
        (AUD, "python3 tools/agents/mission.py scope M-0001 --ref origin/agent/M-0001-x", False),
        (AUD, "cd /w && python3 tools/agents/preserve.py origin/develop origin/agent/M-0001-x", False),
        (A, "python3 tools/agents/mission.py status M-0001 ACCEPTÉE", True),
        (A, "python3 tools/agents/preserve.py a b > tools/agents/preserve.py", True),
        (AUD, "git commit -m x", True),
        (AUD, "gh api repos/o/r/pulls/3/files", False),
        (AUD, "gh api repos/o/r/issues/3/comments -f body=x", True),
        (AUD, "gh run rerun 5", True),
        (D, "NF_DIRECTOR=1 git push origin develop", False),
        (D, "git push -f origin main", True),
        (D, "git push origin :develop", True),
        (D, "gh api -X PUT repos/o/r/pulls/3/merge", False),
    ]
    bad = 0
    for ctx, cmd, should_block in cases:
        r = bash(ctx, cmd)
        if bool(r) != should_block:
            bad += 1
            print(f"ÉCHEC : {ctx.get('agent_type') or 'directeur'} | {cmd!r} -> {r!r}")
    files = [
        (A, "/w/tools/agents/guard.py", True),
        (A, "/w/app/src/nf/windows.rs", False),
        (A, os.path.expanduser("~/.claude/agents/nf-x.md"), True),
        (D, "/w/tools/agents/guard.py", False),
    ]
    for ctx, path, should_block in files:
        r = decide(dict(ctx, tool_name="Edit", tool_input={"file_path": path}))
        if bool(r) != should_block:
            bad += 1
            print(f"ÉCHEC fichier : {path} -> {r!r}")
    if decide(dict(A, tool_name="Agent", tool_input={})) is None:
        bad += 1
        print("ÉCHEC : un agent peut lancer un agent")
    total = len(cases) + len(files) + 1
    print(f"guard.py : {total - bad}/{total} cas conformes")
    return 1 if bad else 0


def main():
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    try:
        data = json.load(sys.stdin)
    except (ValueError, OSError):
        print("garde-fou NIGHTFALL : entrée illisible, appel refusé par prudence", file=sys.stderr)
        sys.exit(2)
    reason = decide(data)
    if reason:
        log(data, reason)
        who = data.get("agent_type") or "Directeur"
        print(f"Refusé par le garde-fou NIGHTFALL ({who}) : {reason}. "
              "Voir docs/agents/ORGANISATION.md §6. Si c'est nécessaire, signale-le dans ton rapport.",
              file=sys.stderr)
        sys.exit(2)
    sys.exit(0)


if __name__ == "__main__":
    main()
