# Generator of NIGHTFALL wallpapers (original work, MPL-2.0). Writes SVG files.
import math, random, sys, os
W, H = 1920, 1080
OUT = sys.argv[1]

def lerp(a, b, t): return a + (b - a) * t
def hexrgb(h): h = h.lstrip('#'); return tuple(int(h[i:i+2], 16) for i in (0, 2, 4))
def mix(c1, c2, t):
    a, b = hexrgb(c1), hexrgb(c2)
    return '#%02x%02x%02x' % tuple(round(lerp(x, y, t)) for x, y in zip(a, b))

def noise1d(seed, n=6):
    r = random.Random(seed)
    waves = [(r.uniform(0, 6.28), r.uniform(0, 6.28), 1.9 ** i, 0.46 ** i) for i in range(n)]
    def f(x):
        return sum(a * (0.65 * math.sin(x * fr * 2.1 + ph) + 0.35 * math.sin(x * fr * 3.3 + ph2)) for ph, ph2, fr, a in waves)
    return f

def ridge(seed, base, amp, step=6, sharp=False, n=5, freq=2.2):
    f = noise1d(seed, n)
    pts = []
    for x in range(-20, W + 21, step):
        v = f(x / W * freq)
        if sharp: v = 0.9 - abs(v) * 1.1
        pts.append((x, base - v * amp))
    d = 'M-20,%d ' % (H + 20) + ' '.join('L%d,%.1f' % p for p in pts) + ' L%d,%d Z' % (W + 20, H + 20)
    return d

def stars(seed, count, ymax, color='#ffffff'):
    r = random.Random(seed)
    out = []
    for _ in range(count):
        x, y = r.uniform(0, W), r.uniform(0, ymax) ** 1.0
        y = ymax * (r.random() ** 1.6)
        s = r.choice([0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.5])
        o = r.uniform(0.25, 0.9) * (1 - y / ymax * 0.7)
        out.append('<circle cx="%.0f" cy="%.0f" r="%.1f" fill="%s" opacity="%.2f"/>' % (x, y, s, color, o))
    return '\n'.join(out)

def svg(body, defs=''):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" preserveAspectRatio="xMidYMid slice">\n'
            '<!-- NIGHTFALL wallpaper, original work (MPL-2.0) -->\n<defs>%s</defs>\n%s\n</svg>\n') % (W, H, defs, body)

def lin(id, stops, x2=0, y2=1):
    s = ''.join('<stop offset="%s" stop-color="%s"/>' % (o, c) for o, c in stops)
    return '<linearGradient id="%s" x1="0" y1="0" x2="%s" y2="%s">%s</linearGradient>' % (id, x2, y2, s)

def rad(id, stops, cx=.5, cy=.5, r=.5):
    s = ''.join('<stop offset="%s" stop-color="%s" stop-opacity="%s"/>' % (o, c, a) for o, c, a in stops)
    return '<radialGradient id="%s" cx="%s" cy="%s" r="%s">%s</radialGradient>' % (id, cx, cy, r, s)

def fog(id, color, a):
    return '<linearGradient id="%s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="%s" stop-opacity="0"/><stop offset=".55" stop-color="%s" stop-opacity="%s"/><stop offset="1" stop-color="%s" stop-opacity="0"/></linearGradient>' % (id, color, color, a, color)

def layer_grad(id, top, bottom):
    return lin(id, [(0, top), (1, bottom)])

walls = {}

# 1. Crêtes : layered mountain ridges at dusk, big soft moon.
def cretes():
    defs = lin('sky', [(0, '#0b1026'), (.5, '#272250'), (.8, '#5e4676'), (1, '#b98088')])
    defs += rad('glow', [(0, '#ffe9d6', .5), (.35, '#f3b9a8', .16), (1, '#f3b9a8', 0)], .5, .5, .5)
    defs += fog('fog', '#e7b4b2', .22)
    layers = [(600, 150, '#9a7795', '#6e5a80'), (680, 130, '#6a5582', '#463c66'), (760, 115, '#463c66', '#2c2849'), (850, 100, '#2c2849', '#1b1934'), (950, 80, '#1b1934', '#0f0e20')]
    for i, (_, _, t, bt) in enumerate(layers):
        defs += layer_grad('l%d' % i, t, bt)
    body = '<rect width="%d" height="%d" fill="url(#sky)"/>' % (W, H)
    body += stars(11, 240, 560)
    body += '<circle cx="1360" cy="400" r="280" fill="url(#glow)"/>'
    body += '<circle cx="1360" cy="400" r="58" fill="#fff4e8" opacity=".96"/>'
    for i, (base, amp, _, _) in enumerate(layers):
        body += '<path d="%s" fill="url(#l%d)"/>' % (ridge(100 + i * 7, base, amp, sharp=i < 2, freq=1.8 + i * .3), i)
        if i < 3:
            body += '<rect x="0" y="%d" width="%d" height="170" fill="url(#fog)"/>' % (base - 40, W)
    return svg(body, defs)

# 2. Dunes : night desert, crescent moon.
def dunes():
    defs = lin('sky', [(0, '#060b1a'), (.6, '#10243d'), (1, '#2c5266')])
    defs += rad('mg', [(0, '#dfe9f0', .25), (1, '#dfe9f0', 0)], .5, .5, .5)
    for i in range(5):
        top = mix('#3d6f86', '#0c1b2b', i / 4); bot = mix('#18374a', '#060d17', i / 4)
        defs += lin('d%d' % i, [(0, top), (1, bot)])
    body = '<rect width="%d" height="%d" fill="url(#sky)"/>' % (W, H)
    body += stars(21, 320, 640)
    body += '<circle cx="420" cy="260" r="190" fill="url(#mg)"/><circle cx="420" cy="260" r="46" fill="#f2f0e6"/><circle cx="440" cy="248" r="44" fill="#08111f"/>'
    for i in range(5):
        base = 650 + i * 95
        f = noise1d(300 + i, 3)
        pts = ['M-20,%d' % (H + 20)]
        for x in range(-20, W + 21, 10):
            v = f(x / W * 1.6)
            pts.append('L%d,%.1f' % (x, base - v * (70 - i * 6)))
        pts.append('L%d,%d Z' % (W + 20, H + 20))
        body += '<path d="%s" fill="url(#d%d)"/>' % (' '.join(pts), i)
    return svg(body, defs)

# 3. Lac : mountains mirrored on still water.
def lac():
    defs = lin('sky', [(0, '#081221'), (.7, '#1d3a5c'), (1, '#4c7898')])
    defs += lin('water', [(0, '#2f5675'), (1, '#060e19')])
    defs += rad('glow', [(0, '#d9ecff', .42), (1, '#d9ecff', 0)], .5, .5, .5)
    defs += '<linearGradient id="fade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".55"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient><mask id="m"><rect y="640" width="1920" height="440" fill="url(#fade)"/></mask>'
    hz = 640
    body = '<rect width="%d" height="%d" fill="url(#sky)"/>' % (W, hz)
    body += stars(31, 200, 420)
    body += '<circle cx="640" cy="250" r="210" fill="url(#glow)"/><circle cx="640" cy="250" r="38" fill="#eef6ff"/>'
    mounts = ''
    for i, (base, amp, col) in enumerate([(470, 160, '#2d4d6b'), (545, 115, '#1c3552'), (615, 60, '#10233a')]):
        f = noise1d(400 + i, 5)
        pts = ['M-20,%d' % hz]
        for x in range(-20, W + 21, 6):
            v = 0.9 - abs(f(x / W * 2.0)) * 1.1 if i == 0 else f(x / W * 2.4)
            pts.append('L%d,%.1f' % (x, min(hz, base - v * amp)))
        pts.append('L%d,%d Z' % (W + 20, hz))
        mounts += '<path d="%s" fill="%s"/>' % (' '.join(pts), col)
    body += mounts
    body += '<rect y="%d" width="%d" height="%d" fill="url(#water)"/>' % (hz, W, H - hz)
    body += '<g mask="url(#m)"><g transform="translate(0,%d) scale(1,-1)">%s</g></g>' % (2 * hz, mounts)
    r = random.Random(5)
    for _ in range(90):
        t = r.random() ** 1.3
        y = hz + 4 + t * (H - hz - 4)
        x = 640 + r.gauss(0, 30 + t * 140)
        w = r.uniform(10, 50) * (0.5 + t)
        body += '<rect x="%.0f" y="%.0f" width="%.0f" height="1.2" rx=".6" fill="#e6f2ff" opacity="%.2f"/>' % (x - w / 2, y, w, r.uniform(.12, .4) * (1 - t * .7))
    body += '<rect y="%d" width="%d" height="1" fill="#a9c4de" opacity=".25"/>' % (hz, W)
    return svg(body, defs)

# 4. Forêt : pine silhouettes layered in fog.
def foret():
    defs = lin('sky', [(0, '#0a1416'), (.6, '#1b3533'), (1, '#4f746b')])
    defs += fog('fog', '#bcd5cb', .16)
    defs += rad('mg', [(0, '#e6f0e6', .32), (1, '#e6f0e6', 0)], .5, .5, .5)
    body = '<rect width="%d" height="%d" fill="url(#sky)"/>' % (W, H)
    body += stars(41, 170, 420)
    body += '<circle cx="1480" cy="290" r="170" fill="url(#mg)"/><circle cx="1480" cy="290" r="34" fill="#eef3e8" opacity=".95"/>'
    r = random.Random(9)
    for layer in range(4):
        base = 700 + layer * 120
        col = mix('#5f857b', '#060d0e', layer / 3)
        g = noise1d(800 + layer, 3)
        trees = ['<rect x="0" y="%d" width="%d" height="%d" fill="%s"/>' % (base, W, H - base, col)]
        x = -40
        while x < W + 40:
            h = (110 + 120 * (0.5 + 0.5 * g(x / W * 2))) * r.uniform(.75, 1.15) * (1 + layer * 0.3)
            w = h * r.uniform(.24, .3)
            tip = base - h
            pts = []
            tiers = 7
            for t in range(1, tiers + 1):
                yy = tip + h * t / tiers
                pts.append((x - w / 2 * t / tiers * 1.12, yy))
                if t < tiers:
                    pts.append((x - w / 2 * t / tiers * 0.72, yy + 2))
            left = ' '.join('L%.0f,%.0f' % p for p in pts)
            right = ' '.join('L%.0f,%.0f' % (2 * x - px, py) for px, py in reversed(pts))
            trees.append('<path d="M%.0f,%.0f %s L%.0f,%.0f %s Z" fill="%s"/>' % (x, tip, left, x, base + 2, right, col))
            x += w * r.uniform(.7, 1.6)
        body += ''.join(trees)
        if layer < 3:
            body += '<rect x="0" y="%d" width="%d" height="200" fill="url(#fog)"/>' % (base - 70, W)
    return svg(body, defs)

# 5. Océan : horizon at night, moon path on the sea.
def ocean():
    defs = lin('sky', [(0, '#050912'), (.65, '#14213f'), (1, '#36507a')])
    defs += lin('sea', [(0, '#1c2f52'), (1, '#03060d')])
    defs += rad('glow', [(0, '#fff3dc', .5), (1, '#fff3dc', 0)], .5, .5, .5)
    hz = 700
    body = '<rect width="%d" height="%d" fill="url(#sky)"/>' % (W, hz)
    body += stars(51, 380, 600)
    body += '<circle cx="960" cy="380" r="240" fill="url(#glow)"/><circle cx="960" cy="380" r="54" fill="#fff6e6"/>'
    body += '<rect y="%d" width="%d" height="%d" fill="url(#sea)"/>' % (hz, W, H - hz)
    r = random.Random(7)
    for _ in range(260):
        t = r.random() ** 1.4
        y = hz + 3 + t * (H - hz)
        spread = 30 + t * 360
        x = 960 + r.gauss(0, spread / 2.2)
        w = r.uniform(6, 30) * (0.4 + t)
        body += '<rect x="%.0f" y="%.0f" width="%.0f" height="%.1f" rx="1" fill="#ffe9c7" opacity="%.2f"/>' % (x - w / 2, y, w, 1 + t * 1.6, r.uniform(.15, .55) * (1 - t * .6))
    body += '<rect y="%d" width="%d" height="1.5" fill="#9fb6d8" opacity=".35"/>' % (hz, W)
    return svg(body, defs)

# 6. Aube : pastel dawn ridges (light).
def aube():
    defs = lin('sky', [(0, '#f4e7dc'), (.55, '#f7d2c0'), (1, '#e9a99a')])
    defs += rad('sun', [(0, '#fff8ef', 1), (.4, '#ffe2c9', .6), (1, '#ffe2c9', 0)], .5, .5, .5)
    defs_extra = ''
    body = '<rect width="%d" height="%d" fill="url(#sky)"/>' % (W, H)
    body += '<circle cx="700" cy="560" r="300" fill="url(#sun)"/>'
    defs += fog('fog', '#fff1e6', .35)
    cols = ['#e3a59c', '#c98d92', '#a9798c', '#836582', '#5d4f70']
    for i, col in enumerate(cols):
        body += '<path d="%s" fill="%s"/>' % (ridge(600 + i * 5, 620 + i * 95, 110 - i * 10, sharp=i < 2, freq=1.6 + i * .3), col)
        if i < 3:
            body += '<rect x="0" y="%d" width="%d" height="160" fill="url(#fog)"/>' % (600 + i * 95, W)
    return svg(body, defs)

# 7. Graphite : calm abstract bands (minimal).
def graphite():
    defs = lin('bg', [(0, '#111317'), (1, '#1c1f26')], 1, 1)
    body = '<rect width="%d" height="%d" fill="url(#bg)"/>' % (W, H)
    for i in range(7):
        f = noise1d(700 + i, 3)
        pts = ['M-20,%d' % (H + 20)]
        for x in range(-20, W + 21, 12):
            pts.append('L%d,%.1f' % (x, 260 + i * 110 - f(x / W * 1.3) * 90))
        pts.append('L%d,%d Z' % (W + 20, H + 20))
        body += '<path d="%s" fill="%s" opacity=".55"/>' % (' '.join(pts), mix('#2a2f3a', '#0c0e12', i / 6))
    return svg(body, defs)

for name, fn in [('cretes', cretes), ('dunes', dunes), ('lac', lac), ('foret', foret), ('ocean', ocean), ('aube', aube), ('graphite', graphite)]:
    open(os.path.join(OUT, name + '.svg'), 'w').write(fn())
    print(name, os.path.getsize(os.path.join(OUT, name + '.svg')))
