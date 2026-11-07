// Sites que le lecteur NIGHTFALL accepte d'ouvrir (navigation de premier niveau).
// Tout le reste est rendu à NIGHTFALL. Les ressources des pages (images, vidéo,
// scripts des CDN de ces services) ne sont pas concernées : seule l'adresse
// affichée dans la fenêtre l'est. Les domaines de connexion de chaque service y
// figurent pour que l'identification se fasse dans le lecteur.
'use strict';

const SERVICES = [
  { name: 'Netflix', domains: ['netflix.com'] },
  { name: 'Disney+', domains: ['disneyplus.com', 'disney.com', 'go.com', 'bamgrid.com'] },
  { name: 'Prime Video', domains: ['primevideo.com', 'amazon.com', 'amazon.fr', 'amazon.de', 'amazon.co.uk', 'amazon.es', 'amazon.it', 'amazon.ca'] },
  { name: 'Canal+', domains: ['canalplus.com', 'canal-plus.com', 'mycanal.fr'] },
  { name: 'Max', domains: ['max.com', 'hbomax.com'] },
  { name: 'Apple TV+', domains: ['tv.apple.com', 'idmsa.apple.com', 'appleid.apple.com'] },
  { name: 'Paramount+', domains: ['paramountplus.com'] },
  { name: 'France.tv', domains: ['france.tv'] },
  { name: 'Molotov', domains: ['molotov.tv'] },
  { name: 'Crunchyroll', domains: ['crunchyroll.com'] },
];

// Hôte égal au domaine ou sous-domaine de celui-ci (jamais « evilnetflix.com »).
function hostMatches(host, domain) {
  return host === domain || host.endsWith('.' + domain);
}

/** Le service dont relève cette adresse, ou null. Seulement https. */
function serviceFor(url) {
  let u;
  try { u = new URL(url); } catch { return null; }
  if (u.protocol !== 'https:' || u.username || u.password || (u.port && u.port !== '443')) return null;
  const host = u.hostname.toLowerCase().replace(/\.$/, '');
  return SERVICES.find((s) => s.domains.some((d) => hostMatches(host, d))) || null;
}

module.exports = { SERVICES, serviceFor, hostMatches };
