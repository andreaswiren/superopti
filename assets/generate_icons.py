"""Render the Signal mark from Lucide audio-lines (see lucide/LICENSE).
Requires Pillow only when regenerating; builds use checked-in assets.
"""
from pathlib import Path
from PIL import Image, ImageDraw
ROOT=Path(__file__).resolve().parent
SIZES=[16,20,24,32,40,48,64,128,256]

def render(size, active=False):
    factor=4
    im=Image.new('RGBA',(size*factor,size*factor))
    d=ImageDraw.Draw(im)
    def box(values): return tuple(round(v*size*factor/256) for v in values)
    d.rounded_rectangle(box((4,4,252,252)),radius=round(56*size*factor/256),fill='#20222e')
    # Lucide audio-lines paths, mapped from its 24-unit grid.
    for x,y,length in [(2,10,3),(6,6,11),(10,3,18),(14,8,7),(18,5,13),(22,10,3)]:
        start=box((32+x*8,32+y*8));end=box((32+x*8,32+(y+length)*8))
        width=max(3,round(16*size*factor/256))
        d.line([start,end],fill='#ed85b5',width=width)
        r=width/2
        for px,py in [start,end]:
            d.ellipse((px-r,py-r,px+r,py+r),fill='#ed85b5')
    if active:
        d.ellipse(box((175,7,253,85)),fill='#20222e')
        d.ellipse(box((186,18,242,74)),fill='#70d7c1')
    return im.resize((size,size),Image.Resampling.LANCZOS)

for name,active in [('superopti',False),('superopti-active',True)]:
    render(256,active).save(ROOT/f'{name}.ico',format='ICO',sizes=[(n,n) for n in SIZES],append_images=[render(n,active) for n in SIZES])
    render(512,active).save(ROOT/f'{name}.png')
ROOT.joinpath('superopti.svg').write_text('''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" role="img" aria-label="SuperOpti Signal waveform icon"><rect x="4" y="4" width="248" height="248" rx="56" fill="#20222e"/><g transform="translate(32 32) scale(8)" fill="none" stroke="#ed85b5" stroke-width="2" stroke-linecap="round"><path d="M2 10v3M6 6v11M10 3v18M14 8v7M18 5v13M22 10v3"/></g></svg>''', encoding='utf-8')
