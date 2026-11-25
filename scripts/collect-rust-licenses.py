"""Collect notices from the locally downloaded, locked Cargo package sources."""
import json, re, shutil, tomllib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
lock=tomllib.loads((root/'src-tauri/Cargo.lock').read_text(encoding='utf-8'))
registry=Path.home()/'.cargo/registry/src'
records=[]
for package in lock['package']:
    if not package.get('source','').startswith('registry+'):continue
    candidates=list(registry.glob(f"*/{package['name']}-{package['version']}"))
    if not candidates:continue
    source=candidates[0]
    manifest=tomllib.loads((source/'Cargo.toml').read_text(encoding='utf-8'))['package']
    destination=root/'licenses/rust'/f"{package['name']}-{package['version']}"
    files=[p for p in source.iterdir() if p.is_file() and re.match(r'^(licen[sc]e|copying|notice)([._-]|$)',p.name,re.I)]
    destination.mkdir(parents=True,exist_ok=True)
    for file in files:shutil.copy2(file,destination/file.name)
    records.append({'name':package['name'],'version':package['version'],'license':manifest.get('license'),'repository':manifest.get('repository'),'licenseFiles':[p.name for p in files]})
(root/'dependencies/rust-licenses.json').write_text(json.dumps(records,indent=2)+'\n',encoding='utf-8')
print(f'Collected notices for {len(records)} locked Rust package sources.')
