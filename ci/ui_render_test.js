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
    if (!ok) failed = true;
    await p.screenshot({ path: `ui-${name}.png` });
    await p.close();
  };
  await check('verrouillage', `http://127.0.0.1:${PORT_LOCKED}/shell.html?mode=top&w=1&private=0`, { width: 1280, height: 820 }, 3500);
  await check('barre-d-outils', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=top&w=1&private=0`, { width: 1280, height: 88 }, 1500);
  await check('nouvel-onglet', `http://127.0.0.1:${PORT_UNLOCKED}/newtab.html`, { width: 1280, height: 760 }, 1000);
  await check('menu', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=menu`, { width: 340, height: 720 }, 1000);
  await check('menu-onglet', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=tabmenu-2-ro`, { width: 290, height: 380 }, 1000);
  await check('recherche-page', `http://127.0.0.1:${PORT_UNLOCKED}/shell.html?mode=popup&w=1&private=0&panel=find`, { width: 400, height: 50 }, 1000);
  await check('parametres', `http://127.0.0.1:${PORT_UNLOCKED}/settings.html`, { width: 1280, height: 760 }, 1000);
  await b.close();
  process.exit(failed ? 1 : 0);
})();
