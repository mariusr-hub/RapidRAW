import json
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed

with open('scratch/all_forks.json', 'r', encoding='utf-8') as f:
    forks = json.load(f)

# Sort by pushed_at descending
forks_pushed = [f for f in forks if f['pushed_at'] > f['created_at']]
forks_pushed.sort(key=lambda x: x['pushed_at'], reverse=True)

print(f"Checking top 80 active forks out of {len(forks_pushed)}...")

def check_fork(f):
    full_name = f['full_name']
    url = f"https://github.com/{full_name}.git"
    try:
        res = subprocess.run(
            ['git', 'ls-remote', '--heads', url],
            capture_output=True,
            text=True,
            timeout=10
        )
        if res.returncode != 0:
            return full_name, None
        lines = res.stdout.strip().splitlines()
        branches = []
        for line in lines:
            parts = line.split()
            if len(parts) == 2:
                sha, ref = parts
                branch_name = ref.replace('refs/heads/', '')
                branches.append((branch_name, sha))
        return full_name, branches
    except Exception as e:
        return full_name, None

results = {}
with ThreadPoolExecutor(max_workers=10) as executor:
    futures = {executor.submit(check_fork, f): f['full_name'] for f in forks_pushed[:80]}
    for future in as_completed(futures):
        fname, branches = future.result()
        if branches:
            results[fname] = branches

print(f"Successfully queried {len(results)} forks.")

# Find interesting branches
keywords = ['hdr', 'pano', 'stitch', 'bracket', 'align', 'deghost', 'blend', 'gpu', 'feat', 'perf', 'photo', 'camera']
interesting = []

for fname, branches in results.items():
    fork_interest = []
    for bname, sha in branches:
        if bname in ['SAM3', 'assets']:
            continue
        # Check if keyword in branch name
        if any(kw in bname.lower() for kw in keywords):
            fork_interest.append((bname, sha))
        elif bname not in ['main', 'master']:
            fork_interest.append((bname, sha))
    if fork_interest:
        interesting.append((fname, fork_interest))

print(f"\n=== Found {len(interesting)} forks with custom or interesting branches ===")
for fname, b_list in sorted(interesting, key=lambda x: x[0]):
    print(f"\n{fname}:")
    for bname, sha in b_list:
        print(f"  - {bname} ({sha[:8]})")
