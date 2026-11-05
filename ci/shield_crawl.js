// Shield audit (mission M-0003): record the requests made by real pages,
// WITHOUT any blocking, with headless Chromium (Playwright). Measurement
// tool only; nothing here is used by the browser.
//
// Usage: node shield_crawl.js PAGES_FILE OUT_DIR
// Writes OUT_DIR/crawl.ndjson (one request per line: page, frameUrl, url,
// cpt) and OUT_DIR/pages.json (per page: final URL, HTTP status, number of
// requests, classes and ids present after load, for generic cosmetic rules).
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

// Playwright resource types -> adblock-rust request types, following what
// CEF reports to NIGHTFALL (fetch() is an XHR for Chromium's loader).
const TYPES = {
  document: 'sub_frame', stylesheet: 'stylesheet', image: 'image', media: 'media', font: 'font',
  script: 'script', texttrack: 'other', xhr: 'xmlhttprequest', fetch: 'xmlhttprequest',
  eventsource: 'other', websocket: 'websocket', manifest: 'other', ping: 'ping', other: 'other',
};

(async () => {
  const [pagesFile, outDir] = process.argv.slice(2);
  const pages = fs.readFileSync(pagesFile, 'utf8').split('\n').map((s) => s.trim()).filter((s) => s && !s.startsWith('#'));
  fs.mkdirSync(outDir, { recursive: true });
  const out = fs.createWriteStream(path.join(outDir, 'crawl.ndjson'));
  const summary = [];
  const browser = await chromium.launch();
  for (const target of pages) {
    const ctx = await browser.newContext({ locale: 'fr-FR', viewport: { width: 1280, height: 820 } });
    const page = await ctx.newPage();
    let n = 0;
    page.on('request', (req) => {
      let frameUrl = '';
      try { frameUrl = req.frame().url(); } catch (e) { /* worker */ }
      if (!frameUrl || frameUrl === 'about:blank') frameUrl = page.url() || target;
      const main = req.isNavigationRequest() && req.frame() === page.mainFrame();
      const cpt = main ? 'main_frame' : (TYPES[req.resourceType()] || 'other');
      out.write(JSON.stringify({ page: target, frameUrl, url: req.url(), cpt }) + '\n');
      n++;
    });
    const rec = { page: target, finalUrl: '', status: 0, requests: 0, classes: [], ids: [], error: '' };
    try {
      const resp = await page.goto(target, { waitUntil: 'load', timeout: 45000 });
      rec.status = resp ? resp.status() : 0;
      await page.waitForTimeout(4000);
      await page.mouse.wheel(0, 4000);
      await page.waitForTimeout(3000);
      rec.finalUrl = page.url();
      const ci = await page.evaluate(() => {
        const c = new Set(); const i = new Set();
        for (const el of document.querySelectorAll('[class],[id]')) {
          if (el.id) i.add(el.id);
          for (const k of el.classList) c.add(k);
        }
        return { c: [...c].slice(0, 5000), i: [...i].slice(0, 5000) };
      });
      rec.classes = ci.c; rec.ids = ci.i;
    } catch (e) {
      rec.error = String(e.message || e).split('\n')[0];
      rec.finalUrl = page.url();
    }
    rec.requests = n;
    summary.push(rec);
    console.log(`${target} -> ${rec.finalUrl} HTTP ${rec.status}, ${n} requêtes, ${rec.classes.length} classes, ${rec.ids.length} id ${rec.error}`);
    await ctx.close();
  }
  await browser.close();
  await new Promise((r) => out.end(r));
  fs.writeFileSync(path.join(outDir, 'pages.json'), JSON.stringify(summary));
})();
