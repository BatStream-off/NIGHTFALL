'use strict';
const RISK = { low: ['ok', 'Risque faible'], medium: ['warn', 'Risque moyen'], high: ['danger', 'Risque élevé'], critical: ['danger', 'Risque CRITIQUE'] };

async function load() {
  const r = await api('extensions.list');
  const list = $('#list');
  list.textContent = '';
  if (!check(r)) return;
  if (!r.extensions.length) { list.append(el('li', { class: 'empty', text: 'Aucune extension installée' })); return; }
  for (const e of r.extensions) {
    const a = e.assessment;
    const [cls, label] = RISK[a.overall] || ['', a.overall];
    const details = el('ul', { class: 'list', style: 'margin-top:6px' }, a.findings.filter((f) => f.risk !== 'low').map((f) => el('li', {}, el('code', { text: f.permission }), el('span', { class: 'sub grow', text: f.explanation }))));
    list.append(el('li', { style: 'display:block' },
      el('div', { class: 'row' }, el('b', { class: 'grow', text: `${a.name} ${a.version}` }), el('span', { class: 'badge', text: 'MV' + a.manifest_version }), el('span', { class: 'badge ' + cls, text: label })),
      details));
  }
}
$('#manage').addEventListener('click', () => api('extensions.manage'));
$('#refresh').addEventListener('click', load);
load();
