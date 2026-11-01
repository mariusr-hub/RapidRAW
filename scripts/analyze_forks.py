import urllib.request
import urllib.parse
import json
import time

def github_get(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'})
    try:
        with urllib.request.urlopen(req) as resp:
            rate_rem = resp.headers.get('x-ratelimit-remaining')
            data = json.loads(resp.read().decode('utf-8'))
            return data, rate_rem
    except urllib.error.HTTPError as e:
        print(f"HTTPError {e.code} for {url}: {e.read().decode('utf-8', errors='ignore')[:300]}")
        return None, 0
    except Exception as e:
        print(f"Error for {url}: {e}")
        return None, 0

print("=== 1. SEARCHING PULL REQUESTS IN CYBERTIMON/RAPIDRAW ===")
queries = [
    'repo:CyberTimon/RapidRAW is:pr hdr',
    'repo:CyberTimon/RapidRAW is:pr panorama OR pano',
    'repo:CyberTimon/RapidRAW is:pr stitch OR bracket'
]

seen_prs = set()
for q in queries:
    url = f"https://api.github.com/search/issues?q={urllib.parse.quote(q)}&per_page=100"
    data, rem = github_get(url)
    print(f"Query '{q}': remaining rate limit = {rem}")
    if data and 'items' in data:
        for item in data['items']:
            num = item['number']
            if num in seen_prs:
                continue
            seen_prs.add(num)
            print(f"PR #{num} [{item['state']}] by {item['user']['login']}: {item['title']}")
            print(f"   Created: {item['created_at']}, Closed: {item.get('closed_at')}")
            print(f"   URL: {item['html_url']}")

print(f"\nTotal distinct PRs found: {len(seen_prs)}")
