import subprocess

res = subprocess.run(['git', 'show', 'hogar1977/panorama:src-tauri/src/panorama_utils/stitch/camera.rs'], capture_output=True, text=True, errors='ignore')
lines = res.stdout.splitlines()
print(f"Total lines: {len(lines)}")
for i, line in enumerate(lines[80:200], start=81):
    print(f"{i:3d}: {line}")
