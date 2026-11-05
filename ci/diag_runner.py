"""Mission M-0002 (CI only): runs NIGHTFALL's diagnostic scenario, samples
memory when the scenario asks for it, then measures start-up time.

usage: diag_runner.py <nightfall binary (selftest build)> <work dir>

The binary must be built with the `selftest` feature. Prints every `DIAG`
line of the browser's log, `MEM` and `DEMARRAGE` lines, then `RESUME` lines
(median, quartiles, min, max per group). Always exits 0: it measures, it
does not judge (the self test and the other CI steps do).
"""
import os
import re
import shutil
import statistics
import subprocess
import sys
import time

import psutil

# Windows consoles default to cp1252: the log contains "←", "⌘"...
try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except AttributeError:
    pass

BIN = sys.argv[1]
WORK = sys.argv[2]
WIN = sys.platform.startswith("win")
SCENARIO_TIMEOUT = 50 * 60
STALL_TIMEOUT = 240


def say(line):
    print(line, flush=True)


def tree(pid):
    try:
        p = psutil.Process(pid)
        return [p] + p.children(recursive=True)
    except psutil.Error:
        return []


def memory(pid):
    """(processes, RSS MiB, USS MiB or None, private MiB or None) of the tree."""
    n, rss, uss, priv, uss_ok, priv_ok = 0, 0, 0, 0, True, True
    for q in tree(pid):
        try:
            mi = q.memory_full_info()
            rss += mi.rss
            if hasattr(mi, "uss"):
                uss += mi.uss
            else:
                uss_ok = False
            if hasattr(mi, "private"):
                priv += mi.private
            else:
                priv_ok = False
            n += 1
        except psutil.Error:
            try:
                rss += q.memory_info().rss
                n += 1
                uss_ok = False
            except psutil.Error:
                pass
    mib = 1024 * 1024
    return n, rss / mib, (uss / mib if uss_ok else None), (priv / mib if priv_ok and WIN else None)


def by_type(pid):
    """Process count and RSS MiB per Chromium process type (--type=...)."""
    out = {}
    for q in tree(pid):
        try:
            cmd = " ".join(q.cmdline())
            m = re.search(r"--type=([a-z-]+)", cmd)
            kind = m.group(1) if m else "principal"
            if kind == "utility":
                m2 = re.search(r"--utility-sub-type=([\w.]+)", cmd)
                kind = "utility:" + (m2.group(1).split(".")[-1] if m2 else "?")
            n, rss = out.get(kind, (0, 0))
            out[kind] = (n + 1, rss + q.memory_info().rss / (1024 * 1024))
        except psutil.Error:
            pass
    return " ".join(f"{k}={n}/{rss:.0f}Mo" for k, (n, rss) in sorted(out.items()))


def kill(proc):
    procs = tree(proc.pid)
    for q in procs:
        try:
            q.kill()
        except psutil.Error:
            pass
    psutil.wait_procs(procs, timeout=20)
    if WIN:
        subprocess.run(["taskkill", "/F", "/IM", "nightfall.exe"], capture_output=True)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass


class LogTail:
    def __init__(self, path):
        self.path, self.pos, self.rest = path, 0, ""

    def lines(self):
        try:
            with open(self.path, "rb") as f:
                f.seek(self.pos)
                data = f.read()
        except OSError:
            return []
        self.pos += len(data)
        text = self.rest + data.decode("utf-8", errors="replace")
        parts = text.split("\n")
        self.rest = parts.pop()
        return [p.rstrip("\r") for p in parts]


def fmt(v):
    return "-" if v is None else f"{v:.0f}"


def run_scenario(profile):
    env = dict(os.environ, NIGHTFALL_SELFTEST="diag", NIGHTFALL_PROFILE_DIR=profile)
    out = open(os.path.join(WORK, "diag.out"), "wb")
    proc = subprocess.Popen([BIN], env=env, stdout=out, stderr=subprocess.STDOUT)
    tail = LogTail(os.path.join(profile, "logs", "nightfall.log"))
    lines, done, start, last_diag = [], False, time.time(), time.time()
    while time.time() - start < SCENARIO_TIMEOUT:
        if time.time() - last_diag > STALL_TIMEOUT:
            say(f"DIAG-RUNNER scenario bloque : aucune ligne DIAG depuis {STALL_TIMEOUT} s")
            break
        for line in tail.lines():
            if "DIAG " in line:
                last_diag = time.time()
            if not any(k in line for k in ("DIAG ", "SELFTEST", "ERREUR INTERNE", "réentrant", "refus")):
                continue
            say(line)
            lines.append(line)
            m = re.search(r"DIAG mem-demande etiquette=(\S+)", line)
            if m:
                # Three samples 1.5 s apart, the median of each column is kept.
                samples = []
                for _ in range(3):
                    time.sleep(1.5)
                    samples.append(memory(proc.pid))

                def col(i):
                    vals = [s[i] for s in samples]
                    return statistics.median(vals) if all(v is not None for v in vals) else None

                mem_line = f"MEM etiquette={m.group(1)} processus={fmt(col(0))} rss_mo={fmt(col(1))} uss_mo={fmt(col(2))} prive_mo={fmt(col(3))} par_type: {by_type(proc.pid)}"
                say(mem_line)
                lines.append(mem_line)
            if "DIAG FIN" in line:
                done = True
        if done or proc.poll() is not None:
            break
        time.sleep(0.2)
    if done:
        try:
            proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            pass
    else:
        say(f"DIAG-RUNNER scenario non termine (code={proc.poll()}, {time.time() - start:.0f} s)")
    for line in tail.lines():
        if "DIAG " in line or "ERREUR INTERNE" in line:
            say(line)
            lines.append(line)
    kill(proc)
    out.close()
    try:
        with open(os.path.join(WORK, "diag.out"), "rb") as f:
            text = f.read().decode("utf-8", errors="replace").splitlines()
        say("--- fin de la sortie du processus ---")
        for line in text[-30:]:
            say(line)
    except OSError:
        pass
    return lines


def startup(profile, label, runs, fresh):
    results = []
    for i in range(runs):
        prof = os.path.join(WORK, f"neuf-{i}") if fresh else profile
        env = dict(os.environ, NIGHTFALL_SELFTEST="diag-demarrage", NIGHTFALL_PROFILE_DIR=prof)
        log = os.path.join(prof, "logs", "nightfall.log")
        tail = LogTail(log)
        if os.path.exists(log):
            tail.pos = os.path.getsize(log)
        t0 = time.perf_counter()
        proc = subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        ui, marks = None, {}
        while time.perf_counter() - t0 < 90:
            for line in tail.lines():
                m = re.search(r"DIAG demarrage etape=(\S+) ms=([\d.]+)", line)
                if m:
                    marks[m.group(1)] = float(m.group(2))
                m = re.search(r"DIAG demarrage shield_construction_ms=([\d.]+) regles=(\d+)", line)
                if m:
                    marks["shield"] = float(m.group(1))
                    marks["regles"] = int(m.group(2))
                if "interface prête" in line and ui is None:
                    ui = (time.perf_counter() - t0) * 1000
            if ui is not None or proc.poll() is not None:
                break
            time.sleep(0.02)
        time.sleep(4)
        n, rss, uss, priv = memory(proc.pid)
        kill(proc)
        time.sleep(2)
        line = (f"DEMARRAGE profil={label} essai={i} interface_ms={fmt(ui)} interne_interface_ms={fmt(marks.get('interface_prete'))} "
                f"coeur_ms={fmt(marks.get('coeur_pret'))} contexte_cef_ms={fmt(marks.get('contexte_cef'))} shield_ms={fmt(marks.get('shield'))} "
                f"regles={marks.get('regles', '-')} processus={n} rss_mo={fmt(rss)} uss_mo={fmt(uss)} prive_mo={fmt(priv)}")
        say(line)
        results.append(line)
    return results


def kv(line):
    return dict(re.findall(r"(\w+)=(\S+)", line))


def num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def stats(values):
    v = sorted(x for x in values if x is not None)
    if not v:
        return "n=0"
    q = statistics.quantiles(v, n=4) if len(v) >= 2 else [v[0], v[0], v[0]]
    d = 2 if v[-1] < 10 else 0  # sub-millisecond values (Shield checks)
    return f"n={len(v)} mediane={statistics.median(v):.{d}f} q1={q[0]:.{d}f} q3={q[2]:.{d}f} min={v[0]:.{d}f} max={v[-1]:.{d}f}"


def summary(lines):
    say("=== RESUME (M-0002) ===")
    groups = {}
    for line in lines:
        if "DIAG nav " in line:
            d = kv(line.split("DIAG nav ", 1)[1])
            groups.setdefault((d.get("page"), d.get("config")), []).append(d)
    for (page, cfg), rows in sorted(groups.items()):
        ok = [r for r in rows if r.get("charge") == "1"]
        say(f"RESUME nav page={page} config={cfg} charges={len(ok)}/{len(rows)} total[{stats([num(r.get('total_ms')) for r in ok])}] "
            f"load[{stats([num(r.get('load_ms')) for r in ok])}] fcp[{stats([num(r.get('fcp_ms')) for r in ok])}] lcp[{stats([num(r.get('lcp_ms')) for r in ok])}] "
            f"ttfb[{stats([num(r.get('ttfb_ms')) for r in ok])}] shield_ms[{stats([num(r.get('shield_total_ms')) for r in ok])}] "
            f"bloquees[{stats([num(r.get('shield_bloquees')) for r in ok])}] push_tabs[{stats([num(r.get('push_tabs')) for r in ok])}]")
    closes = {}
    for line in lines:
        if "DIAG fermeture " in line:
            d = kv(line.split("DIAG fermeture ", 1)[1])
            closes.setdefault((d.get("mode"), d.get("cible"), d.get("appui_ms")), []).append(d)
    for key, rows in sorted(closes.items()):
        ok = sum(1 for r in rows if r.get("ferme") == "1")
        kept = [r.get("actif_conserve") for r in rows if r.get("ferme") == "1" and r.get("actif_conserve") in ("0", "1")]
        say(f"RESUME fermeture mode={key[0]} cible={key[1]} appui_ms={key[2]} fermes={ok}/{len(rows)} "
            f"actif_conserve={kept.count('1')}/{len(kept)} tab_activate[{stats([num(r.get('tab_activate_ms')) for r in rows])}] "
            f"premier_push_tabs[{stats([num(r.get('premier_push_tabs_ms')) for r in rows])}]")
    lat = {}
    for line in lines:
        if "DIAG latence " in line:
            d = kv(line.split("DIAG latence ", 1)[1])
            lat.setdefault(d.get("quoi"), []).append(d)
    for what, rows in sorted(lat.items()):
        cols = [k for k in rows[0] if k.endswith("_ms")]
        say(f"RESUME latence quoi={what} " + " ".join(f"{c}[{stats([num(r.get(c)) for r in rows])}]" for c in cols))
    starts = {}
    for line in lines:
        if line.startswith("DEMARRAGE "):
            d = kv(line)
            starts.setdefault(d.get("profil"), []).append(d)
    for prof, rows in sorted(starts.items()):
        say(f"RESUME demarrage profil={prof} interface[{stats([num(r.get('interface_ms')) for r in rows])}] "
            f"shield[{stats([num(r.get('shield_ms')) for r in rows])}] rss_mo[{stats([num(r.get('rss_mo')) for r in rows])}] "
            f"processus[{stats([num(r.get('processus')) for r in rows])}]")


def main():
    if os.path.exists(WORK):
        shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK, exist_ok=True)
    profile = os.path.join(WORK, "profil-diag")
    lines = []
    try:
        lines += run_scenario(profile)
    except Exception as e:  # measurements only: report and go on
        say(f"DIAG-RUNNER erreur scenario : {e!r}")
    try:
        lines += startup(profile, "neuf", 5, fresh=True)
        lines += startup(profile, "existant", 6, fresh=False)
    except Exception as e:
        say(f"DIAG-RUNNER erreur demarrage : {e!r}")
    summary(lines)
    return 0


if __name__ == "__main__":
    sys.exit(main())
