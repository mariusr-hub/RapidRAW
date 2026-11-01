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

show_git_file('KaiserRuben/hdr-export', 'src-tauri/src/hdr.rs', 90)
