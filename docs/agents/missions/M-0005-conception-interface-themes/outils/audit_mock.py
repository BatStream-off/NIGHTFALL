"""Audit M-0005 : maquette de l'API interne avec des données réalistes pour
rendre toutes les pages d'interface (réutilise ci/ui_mock_server.py)."""
import json, os, sys, time
src = open(sys.argv[1], encoding='utf-8').read()
src = src[:src.rindex('http.server.ThreadingHTTPServer')]
exec(src)
NOW = int(time.time())
EXTRA = {
  'history.query': {"ok": True, "entries": [
    {"id": i, "url": u, "title": t, "last_visit": NOW - i * 1800, "visit_count": (7 * i) % 9 + 1} for i, (u, t) in enumerate([
      ("https://fr.wikipedia.org/wiki/Cr%C3%A9puscule", "Crépuscule — Wikipédia"), ("https://www.lemonde.fr/", "Le Monde.fr - Actualités et Infos en France et dans le monde"),
      ("https://github.com/BatStream-off/NIGHTFALL", "BatStream-off/NIGHTFALL: navigateur axé sur la vie privée"), ("https://www.openstreetmap.org/", "OpenStreetMap"),
      ("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "Vidéo — YouTube"), ("https://developer.mozilla.org/fr/docs/Web/CSS", "CSS | MDN"),
      ("https://www.rtbf.be/", "RTBF Actus"), ("https://duckduckgo.com/?q=cef+views", "cef views at DuckDuckGo")], 1)]},
  'bookmarks.list': {"ok": True, "folders": [{"id": 1, "title": "Travail", "parent_id": None}, {"id": 2, "title": "Lecture", "parent_id": None}],
    "bookmarks": [{"id": 1, "url": "https://github.com/", "title": "GitHub", "folder_id": 1, "created": NOW}, {"id": 2, "url": "https://fr.wikipedia.org/", "title": "Wikipédia", "folder_id": 2, "created": NOW},
                  {"id": 3, "url": "https://www.lemonde.fr/", "title": "Le Monde", "folder_id": None, "created": NOW}, {"id": 4, "url": "https://www.openstreetmap.org/", "title": "OpenStreetMap", "folder_id": None, "created": NOW}]},
  'downloads.list': {"ok": True, "downloads": [
    {"id": 1, "file_name": "rapport-annuel-2026.pdf", "live": False, "speed": 0, "path": "C:\\Users\\Camille\\Downloads\\rapport-annuel-2026.pdf", "url": "https://exemple.fr/rapport.pdf", "state": "complete", "received_bytes": 2400000, "total_bytes": 2400000, "started": NOW - 600, "signature": "signed:Exemple SAS"},
    {"id": 2, "file_name": "installateur.exe", "live": True, "speed": 3200000, "danger": "Fichier exécutable : ouvrez-le seulement si vous faites confiance à sa source.", "path": "C:\\Users\\Camille\\Downloads\\installateur.exe", "url": "https://exemple.fr/i.exe", "state": "in_progress", "received_bytes": 12000000, "total_bytes": 48000000, "started": NOW - 60, "signature": "unsigned"},
    {"id": 3, "file_name": "photo.jpg", "live": False, "speed": 0, "path": "", "url": "https://exemple.fr/p.jpg", "state": "cancelled", "received_bytes": 0, "total_bytes": 300000, "started": NOW - 9000, "signature": None}]},
  'vault.list': {"ok": True, "entries": [{"id": 1, "origin": "https://github.com", "username": "camille", "notes": ""}, {"id": 2, "origin": "https://accounts.exemple.fr", "username": "camille@exemple.fr", "notes": "compte pro"}]},
  'extensions.list': {"ok": True, "extensions": [{"assessment": {"name": "Lecteur RSS", "version": "2.1.0", "manifest_version": 3, "overall": "medium",
     "findings": [{"permission": "tabs", "risk": "medium", "explanation": "Voit l'adresse de tous vos onglets."}, {"permission": "storage", "risk": "low", "explanation": ""}]}}]},
  'security.status': {"ok": True, "status": {
    "lock": {"configured": True, "backend": "TPM 2.0 (Windows Hello désactivé)", "hardwareBacked": True, "autoLockMinutes": 15, "lockOnSleep": True},
    "shield": {"enabled": True, "blockPopups": True, "rules": 187234, "listsCached": True,
      "lists": [{"name": "EasyList", "url": "", "enabled": True, "updated": NOW - 7200}, {"name": "EasyPrivacy", "url": "", "enabled": True, "updated": NOW - 7200},
                {"name": "Liste FR", "url": "", "enabled": True, "updated": NOW - 7200}, {"name": "uBlock filters – Annoyances", "url": "", "enabled": False, "updated": None}],
      "allowlist": ["meet.jit.si"], "customFilters": "", "stats": {"total_blocked": 1532, "popups_blocked": 4, "per_site": {"www.lemonde.fr": 412, "www.youtube.com": 301, "fr.wikipedia.org": 3}}},
    "videoguard": {"config": {"enabled": True, "skip_video_ads": True, "actions": {"sponsor": "skip", "selfpromo": "show", "interaction": "show", "intro": "ignore"}},
      "categories": [["sponsor", "Sponsor"], ["selfpromo", "Autopromotion"], ["interaction", "Rappel d'interaction"], ["intro", "Introduction"]],
      "attribution": "Segments fournis par SponsorBlock (sponsor.ajay.app), sous licence CC BY-NC-SA 4.0."},
    "sessionVault": True, "phishingWarnings": True, "autoPurge": {"enabled": False, "min_independent_critical": 2, "categories": []}, "signals": [],
    "chromium": {"sandbox": True, "siteIsolation": True, "remoteDebugging": False}}},
}
class A(H):
    def do_POST(self):
        cmd = self.path.split('/api/')[-1]
        if cmd in EXTRA:
            n = int(self.headers.get('Content-Length') or 0); self.rfile.read(n)
            body = json.dumps(EXTRA[cmd]).encode()
            self.send_response(200); self.send_header('Content-Type', 'application/json'); self.end_headers(); self.wfile.write(body)
            return
        super().do_POST()
http.server.ThreadingHTTPServer(('127.0.0.1', int(os.environ.get('NF_PORT', '8765'))), A).serve_forever()
