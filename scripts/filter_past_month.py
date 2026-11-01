import json
from datetime import datetime

with open('scratch/all_forks.json', 'r', encoding='utf-8') as f:
    forks = json.load(f)

cutoff = "2026-09-24T00:00:00Z"

# Filter forks pushed on or after 2026-09-24
recent_forks = [f for f in forks if f['pushed_at'] >= cutoff]
recent_forks.sort(key=lambda x: x['pushed_at'], reverse=True)

print(f"Total forks pushed in past month (>= {cutoff}): {len(recent_forks)}")

# Now separate those where pushed_at != created_at (i.e. user pushed commits)
# vs just forked without pushing new commits
pushed_with_work = []
just_forked = []

for f in recent_forks:
    created = f['created_at']
    pushed = f['pushed_at']
    # If created before 2026-09-24, but pushed in past month -> definite activity
    # If created in past month, and pushed > created by more than 2 minutes -> pushed work
    c_dt = datetime.fromisoformat(created.replace('Z', '+00:00'))
    p_dt = datetime.fromisoformat(pushed.replace('Z', '+00:00'))
    diff_secs = (p_dt - c_dt).total_seconds()
    
    if diff_secs > 120 or c_dt < datetime.fromisoformat("2026-09-24T00:00:00+00:00"):
        pushed_with_work.append(f)
    else:
        just_forked.append(f)

print(f"Forks with active pushes in past month: {len(pushed_with_work)}")
print(f"Forks just created without further pushes: {len(just_forked)}")

print("\n--- Active forks in past month (sorted by pushed_at): ---")
for f in pushed_with_work:
    name = f['full_name']
    p = f['pushed_at']
    c = f['created_at']
    def_b = f['default_branch']
    stars = f['stargazers_count']
    print(f"{p} (c: {c}) | {name:38} | def: {def_b:15} | stars: {stars}")
