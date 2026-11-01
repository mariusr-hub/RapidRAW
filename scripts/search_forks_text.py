import json

with open('scratch/all_forks.json', 'r', encoding='utf-8') as f:
    forks = json.load(f)

keywords = ['hdr', 'pano', 'stitch', 'bracket', 'merge', 'photo', 'camera', 'align', 'blend']

print("Matching forks by name or description:")
for f in forks:
    text = (f['name'] + " " + (f['description'] or '')).lower()
    for kw in keywords:
        if kw in text and f['description'] != 'A beautiful, non-destructive, and GPU-accelerated RAW image editor built with performance in mind.':
            print(f"- {f['full_name']}: {f['name']} | desc: {f['description']}")
            break
