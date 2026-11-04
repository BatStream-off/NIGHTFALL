"""Mock of the internal API, to render the interface pages in a plain browser (CI)."""
import json, http.server, os
os.chdir(os.environ['NF_UI_DIR'])
UNLOCKED = os.environ.get('NF_UNLOCKED') == '1'
STATE = {"ok": True, "configured": UNLOCKED, "unlocked": UNLOCKED, "awaitingRecovery": False, "lockStatus": None,
         "splash": True, "verticalTabs": False, "windowId": 1, "private": False, "version": "0.1.0",
         "tagline": "Browse Beyond the Darkness.", "signals": [], "lastError": None}
# ---- Settings page (settings.html): realistic settings.get and update.* answers.
SETTINGS = {"search_engine": "duckduckgo", "search_custom_name": "", "search_custom_url": "", "vertical_tabs": False,
    "auto_lock_minutes": 15, "lock_on_sleep": True, "session_vault": True, "restore_session": True, "devtools_enabled": False,
    "phishing_warnings": True, "wallpaper": "aurore", "shortcuts": [], "sync_folder": None,
    "perf": {"discard_enabled": True, "discard_after_minutes": 30, "battery_saver": True},
    "auto_purge": {"enabled": False, "min_independent_critical": 2, "categories": []},
    "startup_mode": "pages", "startup_pages": ["https://www.lemonde.fr/", "https://fr.wikipedia.org/"],
    "show_home_button": True, "default_zoom": 100, "font_size": 16, "block_third_party_cookies": True,
    "global_privacy_control": True, "https_first": True, "webrtc_ip_protection": True, "secure_dns": "quad9", "secure_dns_custom": "",
    "permission_defaults": {"notifications": "block"},
    "site_permissions": [{"origin": "https://meet.jit.si", "kind": "camera", "allow": True, "updated": 1767000000},
                         {"origin": "https://meet.jit.si", "kind": "microphone", "allow": True, "updated": 1767000000},
                         {"origin": "https://www.openstreetmap.org", "kind": "geolocation", "allow": False, "updated": 1767000000}],
    "permission_resets": [], "download_dir": None, "download_ask": False, "accept_languages": "fr-FR,fr,en-US,en", "spellcheck": True}
UIPREFS = {"splash_enabled": True, "hardware_acceleration": True}
KINDS = [["camera", "la caméra"], ["microphone", "le microphone"], ["geolocation", "votre position"], ["notifications", "les notifications"],
         ["clipboard", "le presse-papiers"], ["downloads", "les téléchargements multiples"],
         ["window_management", "la gestion des fenêtres sur vos écrans"], ["local_fonts", "les polices installées"]]
ENGINES = [["duckduckgo", "DuckDuckGo"], ["brave", "Brave Search"], ["qwant", "Qwant"], ["startpage", "Startpage"], ["ecosia", "Ecosia"], ["google", "Google"], ["bing", "Bing"]]
PREFS = ["webkit.webprefs.default_font_size", "webkit.webprefs.default_fixed_font_size", "profile.cookie_controls_mode",
         "webrtc.ip_handling_policy", "intl.accept_languages", "browser.enable_spellchecking", "spellcheck.use_spelling_service", "dns_over_https.mode", "dns_over_https.templates"]
CHROMIUM = [{"name": n, "label": n, "ok": True, "error": ""} for n in PREFS]
UPDATE = {"ok": True, "current": "0.1.3", "latest": "0.1.4", "available": True, "checking": False, "downloading": False, "progress": 0,
          "readyToInstall": False, "lastCheck": 1767000000, "error": None, "signed": True, "autoCheck": True, "autoInstall": False,
          "releaseUrl": "https://github.com/BatStream-off/NIGHTFALL/releases/tag/v0.1.4", "notInstallable": None}

def settings_cmd(cmd, req):
    """Answers of the settings commands, or None for other commands."""
    if cmd == 'settings.get':
        return {"ok": True, "settings": SETTINGS, "prefs": UIPREFS, "engines": [{"id": i, "name": n, "url": ""} for i, n in ENGINES],
                "permissionKinds": [{"id": i, "label": l} for i, l in KINDS], "platform": "windows", "version": "0.1.3",
                "defaultDownloadDir": "C:\\Users\\Camille\\Downloads", "downloadDir": "C:\\Users\\Camille\\Downloads",
                "spellcheckAvailable": False, "hardwareAccelerationAtStart": True, "chromium": CHROMIUM}
    if cmd == 'settings.set':
        if isinstance(req.get('settings'), dict):
            keep = {k: SETTINGS[k] for k in ('site_permissions', 'permission_resets', 'download_dir')}
            SETTINGS.update(req['settings']); SETTINGS.update(keep)
        if isinstance(req.get('prefs'), dict):
            UIPREFS.update(req['prefs'])
        return {"ok": True, "chromium": CHROMIUM}
    if cmd == 'permissions.forget':
        before = len(SETTINGS['site_permissions'])
        SETTINGS['site_permissions'] = [] if req.get('all') else [p for p in SETTINGS['site_permissions'] if not (p['origin'] == req.get('origin') and p['kind'] == req.get('kind'))]
        return {"ok": True, "removed": before - len(SETTINGS['site_permissions'])}
    if cmd == 'browsing.clear':
        return {"ok": True, "history": 12, "downloads": 2, "sites": 5}
    if cmd == 'update.status':
        return UPDATE
    if cmd == 'update.set':
        UPDATE.update({k: bool(req[k]) for k in ('autoCheck', 'autoInstall') if k in req}); return {"ok": True}
    if cmd == 'update.check':
        UPDATE.update({"lastCheck": 1767003600}); return {"ok": True}
    if cmd == 'update.install':
        return {"ok": True}
    return None

class H(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get('Content-Length') or 0); raw = self.rfile.read(n)
        cmd = self.path.split('/api/')[-1]
        try: req = json.loads(raw or b'{}')
        except ValueError: req = {}
        settings_r = settings_cmd(cmd, req if isinstance(req, dict) else {})
        if settings_r is not None:
            r = settings_r
        elif cmd == 'setup':
            STATE['configured'] = True; STATE['awaitingRecovery'] = True
            r = {"ok": True, "recoveryKey": "ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345-6789"}
        elif cmd == 'recovery.confirm':
            STATE['awaitingRecovery'] = False; STATE['unlocked'] = True; r = {"ok": True}
        elif cmd == 'newtab.data':
            r = {"ok": True, "wallpaper": "city", "shortcuts": [
                {"title": "YouTube", "url": "https://www.youtube.com/"}, {"title": "Wikipédia", "url": "https://fr.wikipedia.org/"},
                {"title": "GitHub", "url": "https://github.com/"}, {"title": "Le Monde", "url": "https://www.lemonde.fr/"}],
                "topSites": [{"title": "RTBF", "url": "https://www.rtbf.be/"}, {"title": "Reddit", "url": "https://www.reddit.com/"},
                             {"title": "OpenStreetMap", "url": "https://www.openstreetmap.org/"}, {"title": "Qwant", "url": "https://www.qwant.com/"}],
                "stats": {"blocked": 735, "bytesSaved": 26738688, "secondsSaved": 37}}
        elif cmd == 'state':
            r = STATE
        else:
            r = {"ok": True}
        body = json.dumps(r).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json'); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(('127.0.0.1', int(os.environ.get('NF_PORT', '8765'))), H).serve_forever()
