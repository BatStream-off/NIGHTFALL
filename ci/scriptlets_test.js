// CI: NIGHTFALL Shield scriptlets added for lot L4, run in a real Chromium page.
// usage: node scriptlets_test.js <dossier scriptlets/>
const { chromium } = require('playwright'); const fs=require('fs');
const D=process.argv[2];
const src=['nf-utils.fn.js','href-sanitizer.js','set-local-storage-item.js','set-session-storage-item.js','remove-cookie.js','set-cookie.js','nowebrtc.js','abort-on-stack-trace.js','adblock-detector-defuser.js'].map(f=>fs.readFileSync(D+f,'utf8')).join('\n');
(async()=>{const b=await chromium.launch();const p=await b.newPage();
await p.goto('http://127.0.0.1:8766/about.html'); // any page of the mock server
await p.addScriptTag({content: src});
const r = await p.evaluate(async()=>{
  const out={};
  document.body.insertAdjacentHTML('beforeend','<a id=l1 class=trk href="https://track.example/r?url=https%3A%2F%2Freal.example%2Fpage">https://real.example/text</a><a id=l2 class=trk2 href="https://track.example/r?u=https%3A%2F%2Fdest.example%2F">x</a><a id=l3 class=trk3 href="https://track.example/r?u=javascript%3Aalert(1)">x</a>');
  nfHrefSanitizer('a.trk'); nfHrefSanitizer('a.trk2','?u'); nfHrefSanitizer('a.trk3','?u');
  await new Promise(r=>setTimeout(r,200));
  out.href1=document.getElementById('l1').href; out.href2=document.getElementById('l2').href; out.href3=document.getElementById('l3').href;
  nfSetLocalStorageItem('consent','accepted'); nfSetLocalStorageItem('evil','<script>'); nfSetSessionStorageItem('n','42');
  out.ls=[localStorage.getItem('consent'), localStorage.getItem('evil'), sessionStorage.getItem('n')];
  nfSetLocalStorageItem('consent','$remove$'); out.ls2=localStorage.getItem('consent');
  document.cookie='trackme=1; path=/'; document.cookie='keep=1; path=/';
  nfRemoveCookie('/^track/'); out.cookies=document.cookie;
  nfSetCookie('cc','accept'); nfSetCookie('bad','<x>'); out.cookies2=document.cookie;
  nfNoWebRtc(); try { const pc=new RTCPeerConnection(); out.rtc = typeof pc.createDataChannel; await pc.createOffer().catch(e=>{out.rtcOffer=e.name}); } catch(e){ out.rtc='throw '+e; }
  window.adsObj={x:1}; nfAbortOnStackTrace('adsObj.x', '');  try { void window.adsObj.x; out.aost='no throw'; } catch(e){ out.aost='throw'; }
  nfAdblockDetectorDefuser(); let notDet=false; window.blockAdBlock.onNotDetected(()=>{notDet=true}); window.blockAdBlock.onDetected(()=>{out.detected=true}); window.blockAdBlock.check();
  await new Promise(r=>setTimeout(r,50)); out.notDetected=notDet; out.fab=typeof window.fuckAdBlock.check;
  return out;});
const want = { href1: 'https://real.example/text', href2: 'https://dest.example/', href3: 'https://track.example/r?u=javascript%3Aalert(1)', ls: ['accepted', null, '42'], ls2: null, cookies: 'keep=1', cookies2: 'keep=1; cc=accept', rtc: 'function', rtcOffer: 'NotSupportedError', aost: 'throw', notDetected: true, fab: 'function' };
let ok = true;
for (const [k, v] of Object.entries(want)) {
  const same = JSON.stringify(r[k]) === JSON.stringify(v);
  console.log((same ? 'OK   ' : 'ÉCHEC') + ' ' + k + ' = ' + JSON.stringify(r[k]));
  ok = ok && same;
}
if (r.detected) { console.log('ÉCHEC onDetected appelé'); ok = false; }
await b.close();
process.exit(ok ? 0 : 1);})();
