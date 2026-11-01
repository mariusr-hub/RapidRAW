import subprocess

res = subprocess.run(['git', 'diff', '--stat', 'origin/main...Gigmaster02/main2'], capture_output=True, text=True, errors='ignore')
print("Gigmaster02 diffstat vs origin/main:")
print(res.stdout)
