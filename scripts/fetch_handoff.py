import urllib.request

url = "https://github.com/user-attachments/files/33363836/PR-1895-HANDOFF.md"
req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
try:
    with urllib.request.urlopen(req) as resp:
        content = resp.read().decode('utf-8', errors='ignore')
        with open('scratch/PR-1895-HANDOFF.md', 'w', encoding='utf-8') as f:
            f.write(content)
        print("Successfully downloaded PR-1895-HANDOFF.md, length:", len(content))
        print("First 2000 chars:")
        print(content[:2000])
except Exception as e:
    print("Error:", e)
