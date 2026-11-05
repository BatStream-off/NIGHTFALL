"""Vérifie les contrastes WCAG 2.2 des thèmes des maquettes (ds.css + themes.css).
Règle proposée pour le moteur : tout texte (text, text-2, text-3, accent-2 utilisé
comme texte, texte sur l'accent, couleurs d'état) >= 4,5:1 sur bg, surface, raised,
overlay ; indicateurs non textuels (accent) >= 3:1 sur bg, surface, raised.
Usage : python3 outils/contraste.py maquettes   (code de sortie 1 si une règle échoue)"""
import re, sys, os
D = sys.argv[1]
def lum(h):
    h = h.lstrip('#'); r, g, b = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    f = lambda c: c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
def cr(a, b):
    x, y = sorted([lum(a), lum(b)], reverse=True); return (x + .05) / (y + .05)
ds = open(os.path.join(D, 'ds.css'), encoding='utf-8').read()
root = dict(re.findall(r'(--[\w-]+):\s*(#[0-9a-fA-F]{6}|var\(--[\w-]+\))', ds.split('* {')[0]))
def resolve(v, env):
    while v.startswith('var('): v = env.get(v[4:-1], root.get(v[4:-1], v))
    return v
themes = {'crepuscule': {}}
for name, body in re.findall(r'\[data-theme="(\w+)"\]\s*\{([^}]*)\}', open(os.path.join(D, 'themes.css'), encoding='utf-8').read()):
    if name != 'prive': themes[name] = dict(re.findall(r'(--[\w-]+):\s*(#[0-9a-fA-F]{6})', body))
fail = 0
for n, over in themes.items():
    env = {**root, **over}
    g = lambda k: resolve(env[k], env)
    surf = [g(k) for k in ('--c-bg', '--c-surface', '--c-raised', '--c-overlay')]
    checks = [(k, min(cr(g(k), s) for s in surf), 4.5) for k in ('--c-text', '--c-text-2', '--c-text-3', '--c-accent-2', '--c-danger', '--c-warn', '--c-ok')]
    checks.append(('--c-on-accent/--c-accent', cr(g('--c-on-accent'), g('--c-accent')), 4.5))
    checks.append(('--c-accent (indicateur)', min(cr(g('--c-accent'), s) for s in surf[:3]), 3.0))
    bad = [f'{k} {v:.2f} < {m}' for k, v, m in checks if v < m]
    fail |= bool(bad)
    print(f"{n:11} {'OK ' if not bad else 'ÉCHEC'} " + ' ; '.join(f'{k.replace("--c-", "")} {v:.2f}' for k, v, _ in checks) + (('  → ' + ', '.join(bad)) if bad else ''))
sys.exit(fail)
