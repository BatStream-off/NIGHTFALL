'use strict';
function dayStart(v) { return v ? Math.floor(new Date(v + 'T00:00:00').getTime() / 1000) : null; }
function dayEnd(v) { return v ? Math.floor(new Date(v + 'T23:59:59').getTime() / 1000) : null; }

async function load() {
  const r = await api('history.query', { text: $('#q').value, from: dayStart($('#from').value), to: dayEnd($('#to').value), limit: 500 });
  const list = $('#list');
  list.textContent = '';
  if (!check(r)) return;
  if (!r.entries.length) { list.append(el('li', { class: 'empty', text: 'Aucune page' })); return; }
  for (const h of r.entries) {
    list.append(el('li', {},
      el('div', { class: 'grow', style: 'min-width:0' },
        el('a', { class: 'title', href: h.url, text: h.title || h.url, style: 'display:block;text-decoration:none;color:var(--text)' }),
        el('div', { class: 'sub', text: `${host(h.url)} · ${fmtDate(h.last_visit)} · ${h.visit_count} visite(s)` })),
      el('button', { class: 'ghost', text: 'Supprimer', onclick: async () => { await api('history.delete', { id: h.id }); load(); } })));
  }
}
let t = null;
$('#q').addEventListener('input', () => { clearTimeout(t); t = setTimeout(load, 200); });
$('#from').addEventListener('change', load);
$('#to').addEventListener('change', load);
const now = () => Math.floor(Date.now() / 1000);
$('#del-hour').addEventListener('click', async () => { check(await api('history.deleteRange', { from: now() - 3600, to: now() }), 'Effacé'); load(); });
$('#del-day').addEventListener('click', async () => { check(await api('history.deleteRange', { from: now() - 86400, to: now() }), 'Effacé'); load(); });
$('#del-all').addEventListener('click', async () => { if (confirm('Effacer tout l\'historique ?')) { check(await api('history.clear'), 'Historique effacé'); load(); } });
load();
