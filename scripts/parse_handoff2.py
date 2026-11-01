with open('scratch/PR-1895-HANDOFF.md', 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

crash_idx = text.find('## 3. Crash and correctness bugs')
mem_idx = text.find('## 4. Memory and performance')
plan_idx = text.find('## Suggested work plan')

def safe_print(s):
    print(s.encode('ascii', errors='replace').decode('ascii'))

print("=== 3. CRASH AND CORRECTNESS BUGS ===")
safe_print(text[crash_idx:crash_idx + 4000])

print("\n=== 4. MEMORY AND PERFORMANCE ===")
safe_print(text[mem_idx:mem_idx + 4000])
