'use strict';
let DATA = { folders: [], bookmarks: [] };

function bookmarkRow(b) {
  const folderSel = el('select', { style: 'width:auto' }, el('option', { value: '', text: '(racine)' }), DATA.folders.map((f) => el('option', { value: f.id, text: f.title })));
  folderSel.value = b.folder_id == null ? '' : String(b.folder_id);
  folderSel.addEventListener('change', async () => {
    await api('bookmarks.update', { id: b.id, title: b.title, url: b.url, folder: folderSel.value ? Number(folderSel.value) : null });
    load();
  });
  return el('li', {},
    el('div', { class: 'grow', style: 'min-width:0' },
      el('a', { class: 'title', href: b.url, text: b.title || b.url, style: 'display:block;text-decoration:none;color:var(--text)' }),
      el('div', { class: 'sub', text: b.url })),
    folderSel,
    el('button', { class: 'ghost', text: 'Renommer', onclick: async () => { const t = prompt('Nouveau nom', b.title); if (t) { await api('bookmarks.update', { id: b.id, title: t, url: b.url, folder: b.folder_id }); load(); } } }),
    el('button', { class: 'ghost', text: 'Supprimer', onclick: async () => { await api('bookmarks.delete', { id: b.id }); load(); } }));
}

function renderFolder(parentId, depth) {
  const box = el('div', { style: depth ? 'margin-left:18px' : '' });
  for (const f of DATA.folders.filter((x) => (x.parent_id ?? null) === parentId)) {
    box.append(el('div', { class: 'row', style: 'margin:10px 0 4px' },
      el('b', { class: 'grow folder', text: f.title }),
      el('button', { class: 'ghost', text: 'Sous-dossier', onclick: async () => { const t = prompt('Nom du sous-dossier'); if (t) { await api('bookmarks.folder.add', { parent: f.id, title: t }); load(); } } }),
      el('button', { class: 'ghost', text: 'Renommer', onclick: async () => { const t = prompt('Nouveau nom', f.title); if (t) { await api('bookmarks.folder.rename', { id: f.id, title: t }); load(); } } }),
      el('button', { class: 'ghost', text: 'Supprimer', onclick: async () => { if (confirm('Supprimer ce dossier et son contenu ?')) { await api('bookmarks.folder.delete', { id: f.id }); load(); } } })));
    box.append(renderFolder(f.id, depth + 1));
  }
  const items = DATA.bookmarks.filter((b) => (b.folder_id ?? null) === parentId);
  if (items.length) box.append(el('ul', { class: 'list' }, items.map(bookmarkRow)));
  return box;
}

async function load() {
  const q = $('#q').value.trim();
  const tree = $('#tree');
  tree.textContent = '';
  if (q) {
    const r = await api('bookmarks.search', { q });
    const all = await api('bookmarks.list');
    DATA.folders = all.folders || [];
    if (!check(r)) return;
    tree.append(r.bookmarks.length ? el('ul', { class: 'list' }, r.bookmarks.map(bookmarkRow)) : el('div', { class: 'empty', text: 'Aucun résultat' }));
    return;
  }
  const r = await api('bookmarks.list');
  if (!check(r)) return;
  DATA = r;
  if (!r.bookmarks.length && !r.folders.length) { tree.append(el('div', { class: 'empty', text: 'Aucun favori. Utilisez l\'étoile de la barre d\'adresse ou Ctrl+D.' })); return; }
  tree.append(renderFolder(null, 0));
}

let t = null;
$('#q').addEventListener('input', () => { clearTimeout(t); t = setTimeout(load, 200); });
$('#add-folder').addEventListener('click', async () => { const title = prompt('Nom du dossier'); if (title) { await api('bookmarks.folder.add', { title }); load(); } });
$('#sync-run').addEventListener('click', async () => {
  const r = await api('sync.run', { folder: $('#sync-folder').value.trim(), passphrase: $('#sync-pass').value });
  if (check(r, `Synchronisé : ${r.added || 0} favori(s) ajouté(s)`)) load();
});
api('settings.get').then((s) => { if (s.ok && s.settings.sync_folder) $('#sync-folder').value = s.settings.sync_folder; });
load();
