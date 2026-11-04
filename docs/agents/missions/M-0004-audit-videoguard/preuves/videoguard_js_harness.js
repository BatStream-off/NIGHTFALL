// Banc de test local du script de contenu de VideoGuard (M-0004). Ecrit pour cette mission, aucun code SponsorBlock.
// Usage : NODE_PATH=$(npm root -g) node docs/agents/missions/M-0004-audit-videoguard/preuves/videoguard_js_harness.js
// Principe : Chromium sans tete, page https://www.youtube.com/watch?v=dQw4w9WgXcQ simulee (interception reseau), element <video> factice
// dont currentTime/duration/playbackRate sont remplaces ; les evenements timeupdate/seeking sont emis a la main.
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
 await p.evaluate(()=>{const v=document.querySelector('video');let t=0,pr=1;Object.defineProperty(v,'currentTime',{get:()=>t,set:x=>{t=x;v.dispatchEvent(new Event('seeking'));}});Object.defineProperty(v,'duration',{get:()=>600});Object.defineProperty(v,'playbackRate',{get:()=>pr,set:x=>{pr=x}});window.tick=(x)=>{t=x;v.dispatchEvent(new Event('timeupdate'));};window.cur=()=>t;});
 await p.evaluate(SRC);
 await p.evaluate(()=>window.__nightfallVideoGuardInit({skipVideoAds:true}));
 const data=(segs,id='dQw4w9WgXcQ')=>p.evaluate(([s,i])=>window.__nightfallVideoGuard({videoId:i,segments:s}),[segs,id]);
 const toast=()=>p.evaluate(()=>{const t=document.getElementById('nf-vg-toast');return t?t.textContent:null});
 await p.waitForTimeout(700); // let attach() run (interval 500ms)
 // A skip
 await data([{category:'sponsor',start:10,end:20,uuid:'a',action:'skip'}]);
 await p.evaluate(()=>window.tick(12));
 rec('A.skip.currentTime',await p.evaluate(()=>window.cur()));
 rec('A.skip.toast',await toast());
 // B revenir
 await p.click('#nf-vg-toast button');
 rec('B.revenir.currentTime',await p.evaluate(()=>window.cur()));
 await p.evaluate(()=>window.tick(12.3));
 rec('B.no_reskip_after_revenir',await p.evaluate(()=>window.cur()));
 // D seek before start then enter again
 await p.evaluate(()=>{window.cur();const v=document.querySelector('video');v.currentTime=5;});
 await p.evaluate(()=>window.tick(11));
 rec('D.reskip_after_seek_back',await p.evaluate(()=>window.cur()));
 // C show action
 await data([{category:'intro',start:0,end:30,uuid:'i',action:'show'}]);
 await p.evaluate(()=>window.tick(1));
 rec('C.show.toast',await toast());
 rec('C.show.currentTime_unchanged',await p.evaluate(()=>window.cur()));
 await p.waitForTimeout(6300);
 rec('C.show.toast_after_6.3s',await toast());
 // E seek back, re-enter: second offer?
 await p.evaluate(()=>{document.querySelector('video').currentTime=40;});
 await p.evaluate(()=>{document.querySelector('video').currentTime=0;});
 await p.evaluate(()=>window.tick(2));
 rec('E.show.second_offer_after_seek',await toast());
 // F overlap chain
 await data([{category:'sponsor',start:10,end:20,uuid:'o1',action:'skip'},{category:'selfpromo',start:19,end:30,uuid:'o2',action:'skip'}]);
 await p.evaluate(()=>window.tick(11));
 rec('F.chain_after_tick1',await p.evaluate(()=>window.cur()));
 await p.evaluate(()=>window.tick(window.cur()+0.01));
 rec('F.chain_after_tick2',await p.evaluate(()=>window.cur()));
 // G segment end tolerance: t in [end-0.3,end)
 await data([{category:'sponsor',start:10,end:20,uuid:'g',action:'skip'}]);
 await p.evaluate(()=>window.tick(19.8));
 rec('G.t=end-0.2_not_skipped',await p.evaluate(()=>window.cur()));
 // H SPA to other video: stale segments must not apply
 await data([{category:'sponsor',start:10,end:20,uuid:'h',action:'skip'}]);
 await p.evaluate(()=>history.pushState({}, '', '/watch?v=aaaaaaaaaaa'));
 await p.evaluate(()=>window.tick(12));
 rec('H.spa_other_video_no_skip',await p.evaluate(()=>window.cur()));
 await p.evaluate(()=>history.pushState({}, '', '/watch?v=dQw4w9WgXcQ'));
 // I ads: playback rate restoration
 await p.evaluate(()=>{const v=document.querySelector('video');v.playbackRate=1.5;});
 rec('I.rate_before',await p.evaluate(()=>document.querySelector('video').playbackRate));
 await p.evaluate(()=>document.getElementById('movie_player').classList.add('ad-showing'));
 await p.waitForTimeout(700);
 rec('I.ad.rate_during',await p.evaluate(()=>document.querySelector('video').playbackRate));
 rec('I.ad.currentTime',await p.evaluate(()=>window.cur()));
 await p.evaluate(()=>document.getElementById('movie_player').classList.remove('ad-showing'));
 await p.waitForTimeout(700);
 rec('I.rate_after_ad',await p.evaluate(()=>document.querySelector('video').playbackRate));
 // K mute / unknown action ignored by script
 await data([{category:'sponsor',start:50,end:60,uuid:'m',action:'mute'}]);
 await p.evaluate(()=>window.tick(52));
 rec('K.unknown_action_mute_noop',await p.evaluate(()=>window.cur()));
 // L two skips in a segment shorter than tick: segment 30-30.5 with ticks 29.9 -> 30.6
 await data([{category:'sponsor',start:30,end:31.2,uuid:'s',action:'skip'}]);
 await p.evaluate(()=>window.tick(29.9));await p.evaluate(()=>window.tick(30.15));
 rec('L.short_seg_skip',await p.evaluate(()=>window.cur()));
 await b.close();
})().catch(e=>{console.log('ERR',e);process.exit(1)});
