import json
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed

with open('scratch/all_forks.json', 'r', encoding='utf-8') as f:
    forks = json.load(f)

cutoff = "2026-09-24T00:00:00Z"
recent_forks = [f for f in forks if f['pushed_at'] >= cutoff]

print(f"Total forks updated in past month: {len(recent_forks)}")

def get_remote_heads(f):
    name = f['full_name']
    url = f"https://github.com/{name}.git"
    try:
        res = subprocess.run(['git', 'ls-remote', '--heads', url], capture_output=True, text=True, timeout=12)
        if res.returncode == 0:
            lines = res.stdout.strip().splitlines()
            heads = []
            for l in lines:
                parts = l.split()
                if len(parts) == 2:
                    heads.append((parts[1].replace('refs/heads/', ''), parts[0]))
            return name, heads
    except:
        pass
    return name, []

results = {}
with ThreadPoolExecutor(max_workers=12) as ex:
    futs = {ex.submit(get_remote_heads, f): f['full_name'] for f in recent_forks}
    for fut in as_completed(futs):
        name, heads = fut.result()
        if heads:
            results[name] = heads

print(f"Queried {len(results)} repositories.")

# Filter for keywords
keywords = ['hdr', 'pano', 'stitch', 'bracket', 'align', 'deghost', 'blend', 'merge', 'photo', 'camera', 'calib']

matched_forks = {}
for name, heads in results.items():
    matched_branches = []
    for bname, sha in heads:
        if bname in ['SAM3', 'assets']:
            continue
        if any(kw in bname.lower() for kw in keywords):
            matched_branches.append((bname, sha))
    if matched_branches:
        matched_forks[name] = matched_branches

print(f"\n=== Forks from past month with HDR/Panorama/Stitch branches ({len(matched_forks)}) ===")
for name, branches in sorted(matched_forks.items()):
    print(f"\n{name}:")
    for b, sha in branches:
        print(f"  - branch: {b} ({sha[:8]})")
