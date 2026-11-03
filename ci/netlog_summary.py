"""Summarise a Chromium NetLog for the CI self test (diagnosis only)."""
import json
import sys

d = json.load(open(sys.argv[1]))
c = d["constants"]
et = {v: k for k, v in c["logEventTypes"].items()}
st = {v: k for k, v in c["logSourceType"].items()}
events = d["events"]
ids = set()
for e in events:
    p = json.dumps(e.get("params") or {})
    if "example.com" in p or "93.184" in p or "96.7." in p or "23.192" in p or "23.215" in p:
        ids.add(e["source"]["id"])
# Include sources bound to those (dependency links).
for e in events:
    p = e.get("params") or {}
    dep = p.get("source_dependency")
    if dep and dep.get("id") in ids:
        ids.add(e["source"]["id"])
    if e["source"]["id"] in ids and dep:
        ids.add(dep.get("id"))
count = 0
for e in events:
    if e["source"]["id"] in ids:
        name = et.get(e["type"], e["type"])
        phase = {0: " ", 1: "+", 2: "-"}.get(e.get("phase"), "?")
        params = json.dumps(e.get("params") or {})[:220]
        print(f'{e["time"]} {st.get(e["source"]["type"], "?")}#{e["source"]["id"]} {phase}{name} {params}')
        count += 1
        if count > 150:
            break
