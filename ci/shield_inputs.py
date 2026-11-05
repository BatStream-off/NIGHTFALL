#!/usr/bin/env python3
"""Inputs of the Shield audit (mission M-0003): filter lists and request set.

Downloads, into OUT_DIR:
  lists/<id>.txt            the list exactly as NIGHTFALL downloads it today
  lists/<id>.resolved.txt   same list with `!#include` directives resolved the
                            way uBlock Origin does (same directory only), when
                            the list has any
  lists.json                manifest: id, url, group, trusted, format, sha256,
                            bytes, includes found / resolved
  requests.json             Brave adblock-rust benchmark request set (pinned
                            commit), one JSON object per line

Measurement tool only: nothing here is used by the browser. Standard library
only. Usage: python3 ci/shield_inputs.py OUT_DIR
"""
import hashlib
import json
import os
import re
import sys
import time
import urllib.parse
import urllib.request

UBO = "https://ublockorigin.github.io/uAssets/filters/"
# Same ids, URLs and trust as `default_lists()` in crates/nf-shield/src/lib.rs.
DEFAULT = [
    ("easylist", "https://easylist.to/easylist/easylist.txt", False, True),
    ("easyprivacy", "https://easylist.to/easylist/easyprivacy.txt", False, True),
    ("liste_fr", "https://easylist-downloads.adblockplus.org/liste_fr.txt", False, True),
    ("ubo_filters", UBO + "filters.txt", True, True),
    ("ubo_quick_fixes", UBO + "quick-fixes.txt", True, True),
    ("ubo_privacy", UBO + "privacy.txt", True, True),
    ("ubo_unbreak", UBO + "unbreak.txt", True, True),
    ("ubo_badware", UBO + "badware.txt", True, True),
    ("fanboy_annoyance", "https://secure.fanboy.co.nz/fanboy-annoyance.txt", False, False),
]
# Candidate lists studied by the audit (not used by NIGHTFALL today).
CANDIDATES = [
    ("adguard_base", "https://filters.adtidy.org/extension/ublock/filters/2_without_easylist.txt", False, "standard"),
    ("adguard_tracking", "https://filters.adtidy.org/extension/ublock/filters/3.txt", False, "standard"),
    ("adguard_french", "https://filters.adtidy.org/extension/ublock/filters/16.txt", False, "standard"),
    ("ubo_url_tracking", UBO + "privacy-removeparam.txt", True, "standard"),
    ("ubo_cookies", UBO + "annoyances-cookies.txt", True, "standard"),
    ("ubo_annoyances", UBO + "annoyances.txt", True, "standard"),
    ("easylist_cookie", "https://secure.fanboy.co.nz/fanboy-cookiemonster_ubo.txt", False, "standard"),
    ("peter_lowe", "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=hosts&showintro=0&mimetype=plaintext", False, "hosts"),
    ("urlhaus", "https://malware-filter.gitlab.io/urlhaus-filter/urlhaus-filter-online.txt", False, "standard"),
]
REQUESTS = "https://raw.githubusercontent.com/brave/adblock-rust/09d297c2c2322fef38e91b59521210479516cb4d/data/requests.json"
UA = "NIGHTFALL-Shield/0.1 (+https://github.com/BatStream-off/NIGHTFALL)"
INCLUDE = re.compile(r"^!#include +(\S+)", re.M)


def fetch(url, tries=3):
    last = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=120) as r:
                return r.read()
        except Exception as e:  # noqa: BLE001 - reported in the manifest
            last = e
            time.sleep(2 * (i + 1))
    raise RuntimeError(f"{url}: {last}")


def resolve_includes(text, base, depth=0):
    """Replace `!#include name` by the included file (same directory only,
    as uBlock Origin does). Returns (text, found, resolved, errors)."""
    found = resolved = 0
    errors = []

    def sub(m):
        nonlocal found, resolved
        found += 1
        target = urllib.parse.urljoin(base, m.group(1))
        if depth > 2 or os.path.dirname(target) != os.path.dirname(base):
            errors.append(f"refusé : {m.group(1)}")
            return m.group(0)
        try:
            inner = fetch(target).decode("utf-8", "replace")
        except RuntimeError as e:
            errors.append(str(e))
            return m.group(0)
        inner, f2, r2, e2 = resolve_includes(inner, target, depth + 1)
        found += f2
        resolved += r2 + 1
        errors.extend(e2)
        return inner + "\n"

    return INCLUDE.sub(sub, text), found, resolved, errors


def main():
    out = sys.argv[1]
    os.makedirs(os.path.join(out, "lists"), exist_ok=True)
    manifest = []
    rows = [(i, u, t, "default", "standard", e) for i, u, t, e in DEFAULT]
    rows += [(i, u, t, "candidate", f, False) for i, u, t, f in CANDIDATES]
    for lid, url, trusted, group, fmt, enabled in rows:
        entry = {"id": lid, "url": url, "trusted": trusted, "group": group, "format": fmt, "enabled_by_default": enabled}
        try:
            raw = fetch(url)
        except RuntimeError as e:
            entry["error"] = str(e)
            manifest.append(entry)
            print(f"ÉCHEC {lid}: {e}", flush=True)
            continue
        text = raw.decode("utf-8", "replace")
        with open(os.path.join(out, "lists", lid + ".txt"), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        entry.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(), fetched=int(time.time()))
        resolved, found, ok, errs = resolve_includes(text, url)
        entry.update(includes_found=found, includes_resolved=ok, include_errors=errs)
        if found:
            with open(os.path.join(out, "lists", lid + ".resolved.txt"), "w", encoding="utf-8", newline="\n") as f:
                f.write(resolved)
            entry["resolved_bytes"] = len(resolved.encode("utf-8"))
        manifest.append(entry)
        print(f"{lid}: {len(raw)} octets, sha256 {entry['sha256'][:16]}, !#include {found} (résolus {ok})", flush=True)
    with open(os.path.join(out, "lists.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    data = fetch(REQUESTS)
    with open(os.path.join(out, "requests.json"), "wb") as f:
        f.write(data)
    print(f"requests.json: {len(data)} octets, sha256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    main()
