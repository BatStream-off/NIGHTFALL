// Shield audit (mission M-0003): reference verdicts of uBlock Origin's
// static network filtering engine for the same lists and requests.
//
// Measurement tool only. It runs uBlock Origin (GPL-3.0) from its own
// checkout, built by uBO's `tools/make-nodejs.sh`; no uBO code is copied
// into NIGHTFALL or this repository.
//
// Usage: node shield_ubo_oracle.mjs UBO_NODEJS_DIR INPUTS_DIR
// Reads INPUTS_DIR/lists.json (+ lists/), requests.json, crawl.ndjson.
// Writes INPUTS_DIR/ubo-<name>.ndjson: one line per input line,
// { i, v, f, q }: v = 0 (no match) / 1 (block) / 2 (exception),
// f = matched filter, q = 1 when `$removeparam` would rewrite the URL.
import fs from 'fs';
import path from 'path';
import { pathToFileURL } from 'url';

const [uboDir, inputs] = process.argv.slice(2);
const { StaticNetFilteringEngine, enableWASM } = await import(pathToFileURL(path.join(uboDir, 'index.js')));
const sfp = await import(pathToFileURL(path.join(uboDir, 'js', 'static-filtering-parser.js')));

// What uBlock Origin itself loads from the 8 lists NIGHTFALL enables:
// `!#include` resolved (by ci/shield_inputs.py) and `!#if` evaluated for
// desktop Chromium.
const ENV = ['chromium', 'ublock', 'user_stylesheet'];
const manifest = JSON.parse(fs.readFileSync(path.join(inputs, 'lists.json'), 'utf8'));
const lists = [];
for (const e of manifest.filter((e) => e.enabled_by_default && !e.error)) {
  const dir = path.join(inputs, 'lists');
  const resolved = path.join(dir, `${e.id}.resolved.txt`);
  const raw = fs.readFileSync(fs.existsSync(resolved) ? resolved : path.join(dir, `${e.id}.txt`), 'utf8');
  lists.push({ name: e.id, raw: sfp.utils.preparser.prune(raw, ENV) });
}
const wasm = await enableWASM().catch(() => false);
const engine = await StaticNetFilteringEngine.create();
let t0 = process.hrtime.bigint();
await engine.useLists(lists);
const buildMs = Number(process.hrtime.bigint() - t0) / 1e6;
console.log(`uBO : ${lists.length} listes compilées en ${buildMs.toFixed(0)} ms (une construction, Node ${process.version}, WASM ${wasm})`);

const TYPES = { document: 'main_frame', main_frame: 'main_frame', sub_frame: 'sub_frame', xhr: 'xmlhttprequest', fetch: 'xmlhttprequest', xmlhttprequest: 'xmlhttprequest' };
for (const name of ['requests.json', 'crawl.ndjson']) {
  const file = path.join(inputs, name);
  if (!fs.existsSync(file)) continue;
  const lines = fs.readFileSync(file, 'utf8').split('\n');
  const out = [];
  const times = [];
  let blocked = 0;
  lines.forEach((line, i) => {
    if (!line.trim()) return;
    let r;
    try { r = JSON.parse(line); } catch (e) { return; }
    const type = TYPES[r.cpt] || (['script', 'image', 'stylesheet', 'font', 'media', 'ping', 'websocket', 'object'].includes(r.cpt) ? r.cpt : 'other');
    const details = { originURL: r.frameUrl, url: r.url, type };
    const s = process.hrtime.bigint();
    let v = 0;
    try { v = engine.matchRequest(details); } catch (e) { v = -1; }
    times.push(Number(process.hrtime.bigint() - s));
    let f = '';
    if (v === 1 || v === 2) { try { f = (engine.toLogData() || {}).raw || ''; } catch (e) { /* none */ } }
    let q = 0;
    if (v !== 1 && r.url.includes('?')) {
      try { const fq = engine.filterQuery(details); if (fq && fq.redirectURL) q = 1; } catch (e) { /* none */ }
    }
    if (v === 1 && type !== 'main_frame') blocked++;
    out.push(JSON.stringify({ i, v, f, q }));
  });
  fs.writeFileSync(path.join(inputs, `ubo-${name.replace(/\.[a-z]+$/, '')}.ndjson`), out.join('\n') + '\n');
  times.sort((a, b) => a - b);
  const p = (x) => (times[Math.min(times.length - 1, Math.round((times.length - 1) * x))] / 1000).toFixed(2);
  console.log(`uBO ${name} : ${out.length} requêtes, ${blocked} bloquées (hors navigations), matchRequest médiane ${p(0.5)} µs, p99 ${p(0.99)} µs, p99,9 ${p(0.999)} µs`);
}
