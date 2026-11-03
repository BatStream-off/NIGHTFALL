'use strict';
let editing = null;

function openForm(entry) {
  editing = entry ? entry.id : null;
  $('#form').hidden = false;
  $('#f-origin').value = entry ? entry.origin : '';
  $('#f-origin').disabled = !!entry;
  $('#f-user').value = entry ? entry.username : '';
  $('#f-pass').value = '';
  $('#f-pass').placeholder = entry ? 'Nouveau mot de passe (requis)' : 'Mot de passe';
  $('#f-notes').value = entry ? entry.notes : '';
}

async function load() {
  const r = await api('vault.list', { q: $('#q').value });
  const list = $('#list');
  list.textContent = '';
  if (!check(r)) return;
  if (!r.entries.length) { list.append(el('li', { class: 'empty', text: 'Aucun identifiant enregistré' })); return; }
  for (const c of r.entries) {
    const shown = el('span', { class: 'mono sub' });
    list.append(el('li', {},
      el('div', { class: 'grow', style: 'min-width:0' }, el('div', { class: 'title', text: c.origin }), el('div', { class: 'sub', text: c.username + (c.notes ? ' · ' + c.notes : '') }), shown),
      el('button', { text: 'Afficher', onclick: async () => {
        const pin = prompt('Code NIGHTFALL requis pour afficher ce mot de passe');
        if (!pin) return;
        const r2 = await api('vault.reveal', { id: c.id, pin });
        if (check(r2)) { shown.textContent = r2.password; setTimeout(() => { shown.textContent = ''; }, 15000); }
      } }),
      el('button', { text: 'Modifier', onclick: () => openForm(c) }),
      el('button', { class: 'danger', text: 'Supprimer', onclick: async () => { if (confirm('Supprimer cet identifiant ?')) { await api('vault.delete', { id: c.id }); load(); } } })));
  }
}

$('#new').addEventListener('click', () => openForm(null));
$('#f-gen').addEventListener('click', async () => { const g = await api('vault.generate', { length: 20, lowercase: true, uppercase: true, digits: true, symbols: true }); if (g.ok) { $('#f-pass').value = g.password; $('#f-pass').type = 'text'; } });
$('#f-save').addEventListener('click', async () => {
  const body = { origin: $('#f-origin').value, username: $('#f-user').value, password: $('#f-pass').value, notes: $('#f-notes').value };
  if (!body.password) { toast('Mot de passe requis', true); return; }
  const r = editing ? await api('vault.update', { id: editing, ...body }) : await api('vault.add', body);
  if (check(r, 'Enregistré (chiffré)')) { $('#form').hidden = true; $('#f-pass').value = ''; $('#f-pass').type = 'password'; load(); }
});
let t = null;
$('#q').addEventListener('input', () => { clearTimeout(t); t = setTimeout(load, 200); });
load();
