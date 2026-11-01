import urllib.request
import json

def get_json(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        return {'error': str(e)}

pr1895_comments = get_json('https://api.github.com/repos/CyberTimon/RapidRAW/issues/1895/comments')
print('=== PR #1895 Comments ===')
if isinstance(pr1895_comments, list):
    for c in pr1895_comments:
        print(f"{c.get('user', {}).get('login')}: {c.get('body')[:300]}...")
else:
    print(pr1895_comments)

pr1895 = get_json('https://api.github.com/repos/CyberTimon/RapidRAW/pulls/1895')
head_sha = pr1895.get('head', {}).get('sha')
print(f'Head sha: {head_sha}')
if head_sha:
    checks = get_json(f'https://api.github.com/repos/CyberTimon/RapidRAW/commits/{head_sha}/check-runs')
    print('=== Check runs ===')
    if isinstance(checks, dict) and 'check_runs' in checks:
        for cr in checks.get('check_runs', []):
            print(f"{cr.get('name')}: status={cr.get('status')}, conclusion={cr.get('conclusion')}")
    else:
        print(checks)
