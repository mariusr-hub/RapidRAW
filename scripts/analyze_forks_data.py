import json

with open('scratch/all_forks.json', 'r', encoding='utf-8') as f:
    forks = json.load(f)

# When someone forks a repo, GitHub initializes pushed_at to the upstream's last push at that moment.
# If pushed_at > created_at (by more than a few seconds), the user actually pushed commits!
actually_pushed = []
for f in forks:
    created = f['created_at']
    pushed = f['pushed_at']
    # allow 60s tolerance for initial fork creation
    if pushed > created:
        actually_pushed.append(f)

print(f"Total forks: {len(forks)}")
print(f"Forks where pushed_at > created_at: {len(actually_pushed)}")

actually_pushed.sort(key=lambda x: x['pushed_at'], reverse=True)
print("\nTop 40 forks with pushed commits:")
for f in actually_pushed[:40]:
    name = f['full_name']
    stars = f['stargazers_count']
    pushed = f['pushed_at']
    created = f['created_at']
    desc = f['description'] or ''
    default_b = f['default_branch']
    print(f"{pushed} (created {created}) | {name:35} | def: {default_b:12} | stars: {stars}")
