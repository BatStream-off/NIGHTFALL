// Banc de test local du script de contenu de VideoGuard (M-0004). Ecrit pour cette mission, aucun code SponsorBlock.
// Usage : NODE_PATH=$(npm root -g) node docs/agents/missions/M-0004-audit-videoguard/preuves/videoguard_js_harness.js
// Principe : Chromium sans tete, page https://www.youtube.com/watch?v=dQw4w9WgXcQ simulee (interception reseau), element <video> factice
// dont currentTime/duration/playbackRate sont remplaces ; les evenements timeupdate/seeking sont emis a la main.
// Limites : ce n'est ni le lecteur YouTube ni CEF ; 'seeking' est emis de facon synchrone a l'affectation de currentTime
// (dans un vrai navigateur il est asynchrone) ; la cadence reelle de timeupdate n'est pas simulee ; le lecteur simule ne
// reproduit aucun comportement propre a YouTube (par exemple une eventuelle restauration de la vitesse apres publicite).
// Tentative 2 : cas E refait avec un segment qui ne commence pas a 0 ; cas M (segments imbriques), N (segment a 0),
// O (reinjection pour la meme video), P (noms globaux et elements visibles de la page) ajoutes.
const {chromium}=require('playwright');
const fs=require('fs');
const path=require('path');
const SRC=fs.readFileSync(path.join(__dirname,'..','..','..','..','..','crates','nf-videoguard','src','videoguard.js'),'utf8');
const HTML='<html><head></head><body><div id="movie_player" class="html5-video-player"><video></video></div></body></html>';
const out=[];const rec=(n,v)=>{out.push([n,v]);console.log(n,'=>',JSON.stringify(v));};
(async()=>{
 const b=await chromium.launch();
 const ctx=await b.newContext();
 const p=await ctx.newPage();
 await p.route('https://www.youtube.com/**',r=>r.fulfill({contentType:'text/html',body:HTML}));
 await p.goto('https://www.youtube.com/watch?v=dQw4w9WgXcQ');
 await p.evaluate(()=>{const v=document.querySelector('video');let t=0,pr=1;Object.defineProperty(v,'currentTime',{get:()=>t,set:x=>{t=x;v.dispatchEvent(new Event('seeking'));}});Object.defineProperty(v,'duration',{get:()=>600});Object.defineProperty(v,'playbackRate',{get:()=>pr,set:x=>{pr=x}});window.tick=(x)=>{t=x;v.dispatchEvent(new Event('timeupdate'));};window.cur=()=>t;window.seek=(x)=>{v.currentTime=x;};});
 const globalsBefore=await p.evaluate(()=>Object.getOwnPropertyNames(window));
 await p.evaluate(SRC);
 await p.evaluate(()=>window.__nightfallVideoGuardInit({skipVideoAds:true}));
 const data=(segs,id='dQw4w9WgXcQ')=>p.evaluate(([s,i])=>window.__nightfallVideoGuard({videoId:i,segments:s}),[segs,id]);
 const toast=()=>p.evaluate(()=>{const t=document.getElementById('nf-vg-toast');return t?t.textContent:null});
 const clearToast=()=>p.evaluate(()=>{const t=document.getElementById('nf-vg-toast');if(t)t.remove();});
 const cur=()=>p.evaluate(()=>window.cur());
 const tick=(x)=>p.evaluate((x)=>window.tick(x),x);
 const seek=(x)=>p.evaluate((x)=>window.seek(x),x);
 await p.waitForTimeout(700); // let attach() run (interval 500ms)
 // A skip
 await data([{category:'sponsor',start:10,end:20,uuid:'a',action:'skip'}]);
 await tick(12);
 rec('A.skip.currentTime',await cur());
 rec('A.skip.toast',await toast());
 // B revenir
 await p.click('#nf-vg-toast button');
 rec('B.revenir.currentTime',await cur());
 await tick(12.3);
 rec('B.no_reskip_after_revenir',await cur());
 // D seek before start then enter again
 await seek(5);
 await tick(11);
 rec('D.reskip_after_seek_back',await cur());
 // C show action
 await data([{category:'intro',start:0,end:30,uuid:'i',action:'show'}]);
 await tick(1);
 rec('C.show.toast',await toast());
 rec('C.show.currentTime_unchanged',await cur());
 await p.waitForTimeout(6300);
 rec('C.show.toast_after_6.3s',await toast());
 // E (refait) : segment show 10-40 (ne commence pas a 0), entree a 12, recul a 5 (avant le debut), nouvelle entree a 12
 await data([{category:'intro',start:10,end:40,uuid:'e',action:'show'}]);
 await clearToast();
 await tick(12);
 rec('E.show.first_offer',await toast());
 await clearToast();
 await seek(5);
 await tick(6);
 await tick(12);
 rec('E.show.second_offer_after_seek_before_start',await toast());
 // F overlap chain
 await data([{category:'sponsor',start:10,end:20,uuid:'o1',action:'skip'},{category:'selfpromo',start:19,end:30,uuid:'o2',action:'skip'}]);
 await tick(11);
 rec('F.chain_after_tick1',await cur());
 await p.evaluate(()=>window.tick(window.cur()+0.01));
 rec('F.chain_after_tick2',await cur());
 // G segment end tolerance: t in [end-0.3,end)
 await data([{category:'sponsor',start:10,end:20,uuid:'g',action:'skip'}]);
 await tick(19.8);
 rec('G.t=end-0.2_not_skipped',await cur());
 // H SPA to other video: stale segments must not apply
 await data([{category:'sponsor',start:10,end:20,uuid:'h',action:'skip'}]);
 await p.evaluate(()=>history.pushState({}, '', '/watch?v=aaaaaaaaaaa'));
 await tick(12);
 rec('H.spa_other_video_no_skip',await cur());
 await p.evaluate(()=>history.pushState({}, '', '/watch?v=dQw4w9WgXcQ'));
 // I ads: playback rate restoration
 await p.evaluate(()=>{const v=document.querySelector('video');v.playbackRate=1.5;});
 rec('I.rate_before',await p.evaluate(()=>document.querySelector('video').playbackRate));
 await p.evaluate(()=>document.getElementById('movie_player').classList.add('ad-showing'));
 await p.waitForTimeout(700);
 rec('I.ad.rate_during',await p.evaluate(()=>document.querySelector('video').playbackRate));
 rec('I.ad.currentTime',await cur());
 await p.evaluate(()=>document.getElementById('movie_player').classList.remove('ad-showing'));
 await p.waitForTimeout(700);
 rec('I.rate_after_ad',await p.evaluate(()=>document.querySelector('video').playbackRate));
 // K mute / unknown action ignored by script
 await data([{category:'sponsor',start:50,end:60,uuid:'m',action:'mute'}]);
 await tick(52);
 rec('K.unknown_action_mute_noop',await cur());
 // L short segment with ticks 29.9 -> 30.15
 await data([{category:'sponsor',start:30,end:31.2,uuid:'s',action:'skip'}]);
 await tick(29.9);await tick(30.15);
 rec('L.short_seg_skip',await cur());
 // M segments imbriques A 10-40 et B 15-20 (tries par debut, comme parse_response)
 const nested=[{category:'sponsor',start:10,end:40,uuid:'mA',action:'skip'},{category:'selfpromo',start:15,end:20,uuid:'mB',action:'skip'}];
 await data(nested);
 await seek(0);
 await tick(12);
 rec('M1.nested.entry_at_12',await cur());
 await data(nested);
 await seek(16);
 await tick(16);
 rec('M2.nested.entry_at_16',await cur());
 await tick(20.5);
 rec('M2.nested.after_tick_20.5_no_reskip_of_A',await cur());
 // N segment commencant a 0 : jamais rearme
 await data([{category:'intro',start:0,end:10,uuid:'n0',action:'skip'}]);
 await seek(0);
 await tick(1);
 rec('N.seg0.first_skip',await cur());
 await seek(0);
 await tick(1);
 rec('N.seg0.after_seek_to_0_and_tick_1',await cur());
 await data([{category:'intro',start:5,end:10,uuid:'n5',action:'skip'}]);
 await seek(0);
 await tick(6);
 await seek(0);
 await tick(6);
 rec('N.seg5.control_after_seek_to_0_and_tick_6',await cur());
 // O reinjection pour la meme video (ce que fait client.rs a chaque changement d'adresse vers cette video)
 const one=[{category:'sponsor',start:10,end:20,uuid:'o',action:'skip'}];
 await data(one);
 await seek(0);
 await tick(12);
 rec('O.first_skip',await cur());
 await p.click('#nf-vg-toast button');
 rec('O.revenir',await cur());
 await tick(12.3);
 rec('O.no_reskip_before_reinjection',await cur());
 await data(one);
 await tick(12.6);
 rec('O.after_reinjection_same_video',await cur());
 // P noms globaux et elements ajoutes a la page par VideoGuard (visibles de la page)
 const globalsAfter=await p.evaluate(()=>Object.getOwnPropertyNames(window));
 rec('P.new_globals',globalsAfter.filter(g=>!globalsBefore.includes(g)&&!['tick','cur','seek'].includes(g)).sort());
 rec('P.dom_ids',await p.evaluate(()=>[...document.querySelectorAll('[id^="nf-"]')].map(e=>e.id).sort()));
 rec('P.video_expando',await p.evaluate(()=>document.querySelector('video').__nfVg===true));
 await b.close();
})().catch(e=>{console.log('ERR',e);process.exit(1)});
