#!/bin/sh
# Met en place l'organisation multi-agents de NIGHTFALL dans une session Claude Code.
# À lancer depuis la racine du dépôt : sh tools/agents/install.sh
# - crochets git (pre-commit, pre-push) copiés dans ~/.claude/nightfall/git-hooks et
#   activés pour toutes les copies de travail du dépôt ;
# - garde-fou PreToolUse copié à un emplacement fixe (~/.claude/nightfall/guard.py) ;
# - définitions des agents copiées dans ~/.claude/agents (chargées par Claude Code) ;
# - crochet de garde ajouté aux réglages utilisateur de Claude Code (~/.claude/settings.json).
set -eu
root=$(git rev-parse --show-toplevel)
cd "$root"

mkdir -p "$HOME/.claude/nightfall/git-hooks"
cp tools/agents/git-hooks/* "$HOME/.claude/nightfall/git-hooks/"
chmod +x "$HOME/.claude/nightfall/git-hooks/"*
# Chemin absolu : valable pour toutes les copies de travail, même créées depuis un
# ancien commit qui n'a pas encore les crochets.
git config core.hooksPath "$HOME/.claude/nightfall/git-hooks"
git config remote.origin.fetch '+refs/heads/*:refs/remotes/origin/*'
chmod +x tools/agents/git-hooks/* tools/agents/*.py

mkdir -p "$HOME/.claude/nightfall" "$HOME/.claude/agents"
cp tools/agents/guard.py "$HOME/.claude/nightfall/guard.py"
for f in .claude/agents/nf-*.md; do cp "$f" "$HOME/.claude/agents/"; done

python3 - "$HOME/.claude/settings.json" <<'PY'
import json, os, sys
path = sys.argv[1]
cfg = {}
if os.path.exists(path):
    with open(path, encoding="utf-8") as f:
        cfg = json.load(f)
cmd = 'python3 "$HOME/.claude/nightfall/guard.py"'
pre = cfg.setdefault("hooks", {}).setdefault("PreToolUse", [])
if not any(h.get("command") == cmd for e in pre for h in e.get("hooks", [])):
    pre.append({"matcher": "Bash|Edit|Write|NotebookEdit|MultiEdit|Agent",
                "hooks": [{"type": "command", "command": cmd}]})
with open(path, "w", encoding="utf-8") as f:
    json.dump(cfg, f, indent=2, ensure_ascii=False)
PY

python3 tools/agents/guard.py --self-test
python3 tools/agents/mission.py --self-test
echo "Organisation multi-agents installée. Agents : $(ls .claude/agents | sed 's/\.md$//' | tr '\n' ' ')"
