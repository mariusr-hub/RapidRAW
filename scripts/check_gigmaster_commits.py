import urllib.request
import json

def get_json(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        print(f"Error fetching {url}: {e}")
        return None

branches = ['main2', 'codex/add-hdr-display-capabilities', 'codex/add-output-color-space-option']
for b in branches:
    url = f"https://api.github.com/repos/Gigmaster02/RapidRAW_HDR/commits?sha={b}&per_page=10"
    commits = get_json(url)
    print(f"\n=== Branch {b} ===")
    if commits:
        for c in commits:
            msg = c['commit']['message'].splitlines()[0]
            print(f"  {c['sha'][:8]} - {c['commit']['author']['date']} - {msg}")
