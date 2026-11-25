"""Collect the PE import closure without starting an external process per DLL."""
import argparse, hashlib, json, os, shutil, struct
from collections import deque
from pathlib import Path

def imports(path):
    data=path.read_bytes()
    pe=struct.unpack_from('<I',data,0x3c)[0]
    if data[pe:pe+4]!=b'PE\0\0':raise ValueError(f'Invalid PE: {path}')
    count=struct.unpack_from('<H',data,pe+6)[0]
    optional_size=struct.unpack_from('<H',data,pe+20)[0]
    optional=pe+24
    magic=struct.unpack_from('<H',data,optional)[0]
    directory=optional+(112 if magic==0x20b else 96)
    import_rva=struct.unpack_from('<I',data,directory+8)[0]
    sections=[struct.unpack_from('<IIII',data,optional+optional_size+i*40+8) for i in range(count)]
    def offset(rva):
        for virtual_size,virtual_address,raw_size,raw_address in sections:
            if virtual_address<=rva<virtual_address+max(virtual_size,raw_size):return raw_address+rva-virtual_address
        raise ValueError(f'Invalid import RVA in {path}')
    if not import_rva:return []
    position=offset(import_rva)
    result=[]
    while True:
        descriptor=struct.unpack_from('<IIIII',data,position)
        if not any(descriptor):break
        name=offset(descriptor[3]);end=data.index(b'\0',name)
        result.append(data[name:end].decode('ascii'))
        position+=20
    return result

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--prefix',type=Path,required=True);parser.add_argument('--destination',type=Path,required=True);parser.add_argument('--manifest',type=Path,required=True)
    args=parser.parse_args()
    pending=deque(p for p in args.destination.rglob('*') if p.suffix.lower() in ('.dll','.exe'))
    visited=set();copied=0
    while pending:
        binary=pending.popleft()
        for dll in imports(binary):
            key=dll.lower()
            if key in visited:continue
            visited.add(key)
            source=args.prefix/'bin'/dll
            if source.exists():
                destination=args.destination/dll
                if not destination.exists() or source.stat().st_size!=destination.stat().st_size or source.stat().st_mtime_ns!=destination.stat().st_mtime_ns:
                    shutil.copy2(source,destination);copied+=1
                pending.append(destination)
            elif not (Path(os.environ['WINDIR'])/'System32'/dll).exists() and not key.startswith(('api-ms-','ext-ms-')):
                raise RuntimeError(f'Unresolved import: {dll} ({binary.name})')
    manifest=[{'path':p.relative_to(args.destination).as_posix(),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'size':p.stat().st_size} for p in sorted(args.destination.rglob('*')) if p.is_file()]
    args.manifest.write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    print(f'Validated {len(visited)} DLL imports; copied {copied} libraries; hashed {len(manifest)} files.')

if __name__=='__main__':main()
