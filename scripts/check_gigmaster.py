import urllib.request
import json
import sys

def get_json(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        print(f"Error fetching {url}: {e}")
        return None

# Check Gigmaster02/RapidRAW_HDR
repo_info = get_json("https://api.github.com/repos/Gigmaster02/RapidRAW_HDR")
if repo_info:
    print("=== Gigmaster02/RapidRAW_HDR ===")
    print("Default branch:", repo_info.get('default_branch'))
    print("Created at:", repo_info.get('created_at'))
    print("Pushed at:", repo_info.get('pushed_at'))
    
    # Get branches
    branches = get_json("https://api.github.com/repos/Gigmaster02/RapidRAW_HDR/branches")
    print("Branches:", [b['name'] for b in branches] if branches else [])
    
    # Compare with upstream
    cmp = get_json("https://api.github.com/repos/CyberTimon/RapidRAW/compare/main...Gigmaster02:main")
    if cmp:
        print("Status vs CyberTimon:main ->", cmp.get('status'), "ahead_by:", cmp.get('ahead_by'), "behind_by:", cmp.get('behind_by'))
        for c in cmp.get('commits', [])[:10]:
            print("  Commit:", c['sha'][:8], c['commit']['message'].splitlines()[0])
