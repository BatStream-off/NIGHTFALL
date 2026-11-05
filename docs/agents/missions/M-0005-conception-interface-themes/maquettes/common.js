// Petits utilitaires partagés par les maquettes.
const I = (n, s) => `<i data-i="${n}" ${s ? `data-size="${s}"` : ''}></i>`;
const fav = (c, l, sz = 16) => `<span style="width:${sz}px;height:${sz}px;border-radius:4px;background:${c};display:inline-grid;place-items:center;font:600 ${Math.round(sz * .56)}px/1 var(--f-ui);color:#fff;flex:none">${l}</span>`;
