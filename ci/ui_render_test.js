// CI: render the interface pages in headless Chromium against a mock API and
// fail when a screen shows nothing (the bug of version 0.1.0: blank window).
const { chromium } = require('playwright');
const PORT_LOCKED = 8765;
const PORT_UNLOCKED = 8766;

async function brightShare(page) {
  const buf = await page.screenshot();
  return page.evaluate(async (b64) => {
    const img = new Image();
    img.src = 'data:image/png;base64,' + b64;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.width; c.height = img.height;
    const g = c.getContext('2d'); g.drawImage(img, 0, 0);
    const d = g.getImageData(0, 0, c.width, c.height).data;
    let bright = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 450) bright++;
    return bright / (d.length / 4);
  }, buf.toString('base64'));
}

(async () => {
  const b = await chromium.launch();
  let failed = false;
  const check = async (name, url, viewport, wait) => {
    const p = await b.newPage({ viewport });
    const errors = [];
    p.on('pageerror', (e) => errors.push(e.message));
    await p.goto(url);
    await p.waitForTimeout(wait);
    const share = await brightShare(p);
    const ok = share > 0.0005;
    console.log(`${ok ? 'OK  ' : 'VIDE'} ${name} : contenu visible ${(share * 100).toFixed(2)} %${errors.length ? ' ; erreurs : ' + errors.join(' | ') : ''}`);
    if (!ok || errors.length) failed = true;
    await p.screenshot({ path: `ui-${name}.png` });
    await p.close();
  };
  await check('verrouillage', `http://127.0.0.1:${PORT_LOCKED}/shell.html?mode=top&w=1&private=0`, { width: 1280, height: 820 }, 3500);
  await check('barre-d-outils', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=top&w=1&private=0`, { width: 1280, height: 88 }, 1500);
  await check('nouvel-onglet', `http://127.0.0.1:${PORT_UNLOCKED}/newtab.html`, { width: 1280, height: 760 }, 1000);
  await check('nouvel-onglet-petit', `http://127.0.0.1:${PORT_UNLOCKED}/newtab.html`, { width: 760, height: 560 }, 1000);
  await check('menu', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=menu`, { width: 340, height: 720 }, 1000);
  await check('menu-onglet', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=tabmenu-2-ro`, { width: 290, height: 380 }, 1000);
  await check('recherche-page', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=find`, { width: 400, height: 50 }, 1000);
  await check('parametres', `http://127.0.0.1:${PORT_UNLOCKED}/settings.html`, { width: 1280, height: 760 }, 1000);
  for (const s of ['general', 'confidentialite', 'securite', 'autorisations', 'navigation', 'recherche', 'performances', 'accessibilite', 'avance']) {
    await check('parametres-' + s, `http://127.0.0.1:${PORT_UNLOCKED}/settings.html#${s}`, { width: 1280, height: 760 }, 800);
  }
  await check('menu-fermeture', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=closeconfirm-3`, { width: 340, height: 150 }, 800);

  // Shield popup on a web site, opened at the size of POPUP_SIZE.shield (shell.js).
  // Bug of 0.3.1: nf.css `input[type=checkbox]` (16 px) beat `.toggle`, so the
  // switch was squashed and its knob stuck out; the panel was also taller than
  // its opening size, which cut the « Ouvrir le Security Center » link.
  const shieldSize = await (async () => {
    const p = await b.newPage();
    await p.goto(`http://127.0.0.1:${PORT_UNLOCKED}/shell.js`);
    const m = /shield:\s*\[(\d+),\s*(\d+)\]/.exec(await p.textContent('body'));
    await p.close();
    return m ? { width: Number(m[1]), height: Number(m[2]) } : null;
  })();
  if (!shieldSize) { console.log('ÉCHEC popup Shield : POPUP_SIZE.shield introuvable dans shell.js'); failed = true; }
  for (const [name, off] of [['shield-actif', false], ['shield-desactive', true]]) {
    if (!shieldSize) break;
    const url = `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=shield&mocksite=lemonde.fr${off ? '&mockoff=1' : ''}`;
    await check('popup-' + name, url, shieldSize, 1000);
    const p = await b.newPage({ viewport: shieldSize });
    await p.goto(url);
    await p.waitForSelector('.sh-toggle input.toggle');
    await p.waitForTimeout(300);
    const m = await p.evaluate(() => {
      const box = (e) => { const r = e.getBoundingClientRect(); return { l: r.left, t: r.top, r: r.right, b: r.bottom, w: r.width, h: r.height }; };
      const t = document.querySelector('.sh-toggle input.toggle');
      const after = getComputedStyle(t, '::after');
      const knobLeft = parseFloat(after.left) + (t.checked ? 18 : 0);
      return { toggle: box(t), checked: t.checked, knobRight: knobLeft + parseFloat(after.width),
        inner: box(document.querySelector('#popup-body').firstElementChild), link: box(document.querySelector('#popup-body .m-item')),
        vw: innerWidth, vh: innerHeight };
    });
    const problems = [];
    if (Math.round(m.toggle.w) !== 40 || Math.round(m.toggle.h) !== 22) problems.push(`interrupteur ${m.toggle.w}×${m.toggle.h} au lieu de 40×22`);
    if (m.knobRight > m.toggle.w) problems.push('bouton de l\'interrupteur hors de sa piste');
    if (m.toggle.r > m.vw) problems.push('interrupteur coupé à droite');
    if (m.checked === off) problems.push('état de l\'interrupteur différent de celui du site');
    if (m.inner.b > m.vh + 1) problems.push(`contenu de ${Math.ceil(m.inner.h)} px plus haut que la popup (${m.vh} px)`);
    if (m.link.b > m.vh) problems.push('lien « Ouvrir le Security Center » coupé');
    console.log(`${problems.length ? 'ÉCHEC' : 'OK  '} popup ${name} : ${problems.length ? problems.join(' ; ') : 'interrupteur 40×22, contenu entier'}`);
    if (problems.length) failed = true;
    await p.close();
  }
  await b.close();
  process.exit(failed ? 1 : 0);
})();
