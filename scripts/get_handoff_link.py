import urllib.request
import json

req = urllib.request.Request('https://api.github.com/repos/CyberTimon/RapidRAW/issues/1895/comments', headers={'User-Agent': 'Mozilla/5.0'})
with urllib.request.urlopen(req) as resp:
    comments = json.loads(resp.read().decode('utf-8'))

for c in comments:
    if 'PR-1895-HANDOFF' in c['body']:
        print("=== Full comment with handoff ===")
        print(c['body'])
