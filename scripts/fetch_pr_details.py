import urllib.request
import json
import os

def get_json(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        print(f"Error fetching {url}: {e}")
        return None

# Let's inspect PR #1895, #1329, #1288, #519, #1734
prs = [1895, 1329, 1288, 519, 1734]
for pr_num in prs:
    pr_data = get_json(f"https://api.github.com/repos/CyberTimon/RapidRAW/pulls/{pr_num}")
    if pr_data:
        print(f"\n==================== PR #{pr_num} ====================")
        print(f"Title: {pr_data['title']}")
        print(f"User: {pr_data['user']['login']}")
        print(f"Head: {pr_data['head']['label']} (repo: {pr_data['head']['repo']['full_name'] if pr_data['head']['repo'] else 'deleted'}, branch: {pr_data['head']['ref']})")
        print(f"Base: {pr_data['base']['ref']}")
        print(f"State: {pr_data['state']}, Merged: {pr_data.get('merged')}")
        print(f"Commits: {pr_data['commits']}, Changed files: {pr_data['changed_files']}")
        print(f"Additions: +{pr_data['additions']}, Deletions: -{pr_data['deletions']}")
        print("Body preview:")
        print((pr_data['body'] or 'No description')[:1000])

        # Get list of files
        files_data = get_json(f"https://api.github.com/repos/CyberTimon/RapidRAW/pulls/{pr_num}/files")
        if files_data:
            print("Files touched:")
            for f in files_data[:15]:
                print(f"  {f['status']}: {f['filename']} (+{f['additions']}, -{f['deletions']})")
            if len(files_data) > 15:
                print(f"  ... and {len(files_data) - 15} more files")
