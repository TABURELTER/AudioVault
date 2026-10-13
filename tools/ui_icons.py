"""Export the AudioVault headphone vector mark for native desktop packaging.

Geometry matches public/favicon.svg; drawing at 4x provides clean small icons.
No fonts or external visual assets are required.
"""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
folder = root / 'apps' / 'desktop' / 'src-tauri' / 'icons'
image = Image.new('RGBA', (1024, 1024), (0, 0, 0, 0))
draw = ImageDraw.Draw(image)
factor = 16
def bounds(values):
    return tuple(int(value * factor) for value in values)
draw.rounded_rectangle(bounds((0, 0, 64, 64)), radius=12*factor, fill='#191a1c')
draw.arc(bounds((16,14,48,46)), start=180, end=360, fill='#c9af87', width=3*factor)
draw.line(bounds((16,30,16,36)), fill='#c9af87', width=3*factor)
draw.line(bounds((48,30,48,36)), fill='#c9af87', width=3*factor)
for rect in [(13,31,22,50),(42,31,51,50)]:
    draw.rounded_rectangle(bounds(rect), radius=3*factor, outline='#c9af87', width=3*factor)
for name, size in [('32x32.png',32),('128x128.png',128),('128x128@2x.png',256),('icon.png',512)]:
    image.resize((size,size),Image.Resampling.LANCZOS).save(folder / name)
image.save(folder / 'icon.ico', sizes=[(s,s) for s in [16,24,32,48,64,128,256]])
image.save(folder / 'icon.icns')
for size in [30,44,71,89,107,142,150,284,310]:
    image.resize((size,size),Image.Resampling.LANCZOS).save(folder / f'Square{size}x{size}Logo.png')
image.resize((50,50),Image.Resampling.LANCZOS).save(folder / 'StoreLogo.png')
