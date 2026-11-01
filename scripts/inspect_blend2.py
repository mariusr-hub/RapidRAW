import subprocess

res = subprocess.run(['git', 'show', 'hogar1977/panorama:src-tauri/src/panorama_utils/stitch/blend.rs'], capture_output=True, text=True, errors='ignore')
lines = res.stdout.splitlines()
for i, line in enumerate(lines[80:220], start=81):
    print(f"{i:3d}: {line}")
