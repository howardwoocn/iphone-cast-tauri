"""Generate antialiased radio icons without external design dependencies."""
import math, struct, zlib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
directory=root/'src-tauri/icons'
directory.mkdir(exist_ok=True)

def chunk(name,data):return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)

def render(size):
    # Render each target independently with subpixel coverage, preserving the design.
    # Average covered colours separately to avoid dark transparent-edge fringes.
    samples=8
    pixels=bytearray()
    for y in range(size):
        pixels.append(0)
        for x in range(size):
            covered=accent=0
            for sy in range(samples):
                dy=(y+(sy+.5)/samples)*256/size-128
                for sx in range(samples):
                    dx=(x+(sx+.5)/samples)*256/size-128
                    corner=math.hypot(max(abs(dx)-78,0),max(abs(dy)-78,0))
                    if corner>48:continue
                    covered+=1
                    distance=math.hypot(dx,dy)
                    ring=(abs(distance-48)<4 or abs(distance-76)<4) and abs(dy)<distance*.85
                    if ring or distance<14:accent+=1
            if not covered:
                pixels.extend((0,0,0,0))
            else:
                pixels.extend(tuple(round((a*accent+b*(covered-accent))/covered) for a,b in zip((190,229,150),(21,29,33)))+(round(255*covered/(samples*samples)),))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')

sizes=(16,20,24,32,40,48,64,128,256)
images=[render(size) for size in sizes]
(directory/'icon.png').write_bytes(images[-1])
header=struct.pack('<HHH',0,1,len(images))
offset=6+16*len(images)
for size,png in zip(sizes,images):
    header+=struct.pack('<BBBBHHII',size%256,size%256,0,0,1,32,len(png),offset)
    offset+=len(png)
(directory/'icon.ico').write_bytes(header+b''.join(images))
print('Created antialiased Windows icons:', ', '.join(map(str,sizes)))
