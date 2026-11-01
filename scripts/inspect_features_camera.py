import subprocess

def show_git_file(ref, path, max_lines=100):
    res = subprocess.run(['git', 'show', f'{ref}:{path}'], capture_output=True, text=True, errors='ignore')
    if res.returncode == 0:
        lines = res.stdout.splitlines()
        print(f"=== {path} ({len(lines)} lines) ===")
        for i, line in enumerate(lines[:max_lines]):
            print(f"{i+1:3d}: {line}")
    else:
        print(f"Failed to read {path}: {res.stderr}")

show_git_file('hogar1977/panorama', 'src-tauri/src/panorama_utils/features.rs', 80)
show_git_file('hogar1977/panorama', 'src-tauri/src/panorama_utils/stitch/camera.rs', 80)
