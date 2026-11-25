"""Generate a code-native radio icon without external design dependencies."""
import math, struct, zlib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
directory=root/'src-tauri/icons'
directory.mkdir(exist_ok=True)
size=256
pixels=bytearray()
for y in range(size):
    pixels.append(0)
    for x in range(size):
        dx=x-127.5; dy=y-127.5; distance=math.hypot(dx,dy)
        corner=math.hypot(max(abs(dx)-78,0),max(abs(dy)-78,0))
        if corner>48: pixels.extend((0,0,0,0)); continue
        ring=any(abs(distance-r)<4 and abs(dy)<distance*.85 for r in (48,76))
        center=distance<14
        pixels.extend((190,229,150,255) if ring or center else (21,29,33,255))
def chunk(name,data):return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')
(directory/'icon.png').write_bytes(png)
header=struct.pack('<HHH',0,1,1)+struct.pack('<BBBBHHII',0,0,0,0,1,32,len(png),22)
(directory/'icon.ico').write_bytes(header+png)
print('Created Windows icon.')
