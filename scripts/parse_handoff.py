with open('scratch/PR-1895-HANDOFF.md', 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

# Let's inspect the sections
import re
headers = [line for line in text.splitlines() if line.startswith('## ')]
print("Sections:")
for h in headers:
    print(" ", h)

# Print crash and correctness bugs
crash_idx = text.find('## 3. Crash and correctness bugs')
if crash_idx == -1:
    crash_idx = text.find('Crash and correctness bugs')
print("\n=== Crash and Correctness Bugs excerpt ===")
print(text[crash_idx:crash_idx + 3500])
