"""Archive reviewable source, excluding tools, runtime binaries, keys and caches."""
import json
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED
root=Path(__file__).resolve().parents[1]
version=json.loads((root/'package.json').read_text(encoding='utf-8'))['version']
output=root/'artifacts'/f'iphone-cast-tauri-{version}-source.zip'
output.parent.mkdir(exist_ok=True)
directories=['.cargo','src','src-tauri','native','vendor','scripts','docs','dependencies','licenses']
files=['.gitignore','LICENSE','README.md','package.json','package-lock.json','tsconfig.json','vite.config.ts','index.html']
def include(path):
    parts=path.relative_to(root).parts
    if any(part in ('.git','target','gen','node_modules','__pycache__') for part in parts):return False
    if parts[:3]==('src-tauri','resources','runtime'):return path.name=='.gitkeep'
    return path.is_file()
with ZipFile(output,'w',ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as archive:
    for name in directories:
        for path in sorted((root/name).rglob('*')):
            if include(path):archive.write(path,Path(root.name)/path.relative_to(root))
    for name in files:archive.write(root/name,Path(root.name)/name)
print(f'Source archive: {output}')
