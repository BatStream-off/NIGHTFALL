#!/bin/sh
# Assemble les maquettes : barre latérale commune + contenu de chaque page.
cd "$(dirname "$0")"
gen(){ f=$1; shift; nav=$(cat _nav.txt); for kv in "$@"; do k=${kv%%=*}; nav=$(printf '%s' "$nav" | sed "s/%$k%/on/"); done; nav=$(printf '%s' "$nav" | sed 's/%[A-Z]*%//g')
 { echo '<!doctype html><html lang="fr"><head><meta charset="utf-8"><link rel="stylesheet" href="maquette.css"></head><body><div class="st">'; printf '%s' "$nav"; echo '<main class="main"><div class="wrap">'; cat src/$f.body; echo '</div></main></div></body></html>'; } > $f.html; }
gen 1-security-center SC
gen 2-section-confidentialite CONF
gen 3-autorisations-site SITES
gen 4-evenements SEC
