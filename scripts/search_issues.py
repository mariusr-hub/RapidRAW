import urllib.request
import urllib.parse
import json

def get_json(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        print(f"Error fetching {url}: {e}")
        return None

queries = [
    'repo:CyberTimon/RapidRAW is:issue is:open panorama OR pano OR stitch',
    'repo:CyberTimon/RapidRAW is:issue is:closed panorama OR pano OR stitch',
    'repo:CyberTimon/RapidRAW is:issue is:open hdr OR bracket',
    'repo:CyberTimon/RapidRAW is:issue is:closed hdr OR bracket'
]

for q in queries:
    url = f"https://api.github.com/search/issues?q={urllib.parse.quote(q)}&per_page=20"
    data = get_json(url)
    print(f"\n=== Query: {q} (Total: {data.get('total_count', 0)}) ===")
    if data and 'items' in data:
        for it in data['items'][:10]:
            print(f"  #{it['number']} [{it['state']}] ({it['user']['login']}): {it['title']}")
