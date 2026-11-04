"""Mock of the internal API, to render the interface pages in a plain browser (CI)."""
import json, http.server, os
os.chdir(os.environ['NF_UI_DIR'])
UNLOCKED = os.environ.get('NF_UNLOCKED') == '1'
STATE = {"ok": True, "configured": UNLOCKED, "unlocked": UNLOCKED, "awaitingRecovery": False, "lockStatus": None,
         "splash": True, "verticalTabs": False, "windowId": 1, "private": False, "version": "0.1.0",
         "tagline": "Browse Beyond the Darkness.", "signals": [], "lastError": None}
class H(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get('Content-Length') or 0); self.rfile.read(n)
        cmd = self.path.split('/api/')[-1]
        if cmd == 'setup':
            STATE['configured'] = True; STATE['awaitingRecovery'] = True
            r = {"ok": True, "recoveryKey": "ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345-6789"}
        elif cmd == 'recovery.confirm':
            STATE['awaitingRecovery'] = False; STATE['unlocked'] = True; r = {"ok": True}
        elif cmd == 'newtab.data':
            r = {"ok": True, "wallpaper": "city", "shortcuts": [
                {"title": "YouTube", "url": "https://www.youtube.com/"}, {"title": "Wikipédia", "url": "https://fr.wikipedia.org/"},
                {"title": "GitHub", "url": "https://github.com/"}, {"title": "Le Monde", "url": "https://www.lemonde.fr/"}],
                "topSites": [{"title": "RTBF", "url": "https://www.rtbf.be/"}, {"title": "Reddit", "url": "https://www.reddit.com/"}]}
        elif cmd == 'state':
            r = STATE
        else:
            r = {"ok": True}
        body = json.dumps(r).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json'); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(('127.0.0.1', int(os.environ.get('NF_PORT', '8765'))), H).serve_forever()
