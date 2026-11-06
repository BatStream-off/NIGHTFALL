'use strict';
const STATE = { in_progress: 'En cours', complete: 'Terminé', cancelled: 'Annulé', interrupted: 'Interrompu', paused: 'En pause' };

function sigBadge(sig) {
  if (!sig || sig === 'unknown') return el('span', { class: 'badge', text: 'signature : vérification…' });
  if (sig.startsWith('signed')) return el('span', { class: 'badge ok', text: 'signé' + (sig.length > 7 ? ' · ' + sig.slice(7) : '') });
  if (sig === 'unsigned') return el('span', { class: 'badge warn', text: 'non signé' });
  if (sig.startsWith('invalid')) return el('span', { class: 'badge danger', text: 'signature INVALIDE' });
  return null;
}

async function load() {
  const r = await api('downloads.list');
  const list = $('#list');
  list.textContent = '';
  if (!check(r)) return;
  if (!r.downloads.length) { list.append(el('li', { class: 'empty', text: 'Aucun téléchargement' })); return; }
  for (const d of r.downloads) {
    const pct = d.total_bytes > 0 ? Math.round((d.received_bytes / d.total_bytes) * 100) : null;
    const progress = d.state === 'in_progress'
      ? `${fmtBytes(d.received_bytes)} / ${d.total_bytes > 0 ? fmtBytes(d.total_bytes) : '?'}${pct != null ? ` (${pct} %)` : ''} · ${fmtBytes(d.speed)}/s`
      : `${fmtBytes(d.total_bytes > 0 ? d.total_bytes : d.received_bytes)} · ${fmtDate(d.started)}`;
    list.append(el('li', {},
      el('div', { class: 'grow', style: 'min-width:0' },
        el('div', { class: 'title', text: d.file_name || d.path }),
        el('div', { class: 'sub', text: `${STATE[d.state] || d.state} · ${progress} · ${host(d.url)}` }),
        d.danger ? el('div', { class: 'sub', style: 'color:var(--warn);white-space:normal', text: d.danger }) : null),
      d.state === 'complete' ? sigBadge(d.signature) : null,
      d.live && d.state === 'in_progress' ? el('button', { text: 'Pause', onclick: () => api('downloads.pause', { key: d.key }).then(load) }) : null,
      d.live && d.state === 'in_progress' ? el('button', { text: 'Reprendre', onclick: () => api('downloads.resume', { key: d.key }).then(load) }) : null,
      d.live && d.state === 'in_progress' ? el('button', { class: 'danger', text: 'Annuler', onclick: () => api('downloads.cancel', { key: d.key }).then(load) }) : null,
      d.state === 'complete' ? el('button', { text: 'Afficher dans le dossier', onclick: () => api('downloads.show', { key: d.key }) }) : null));
  }
}
$('#dir').textContent = 'Les fichiers sont enregistrés dans votre dossier Téléchargements.';
$('#clear').addEventListener('click', async () => { check(await api('downloads.clear'), 'Historique effacé'); load(); });
load();
setInterval(load, 1000);
