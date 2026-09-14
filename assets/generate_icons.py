"""Regenerate original SuperOpti icons. Requires Python + Pillow (builds use checked-in assets)."""
from pathlib import Path
from PIL import Image, ImageDraw
ROOT=Path(__file__).resolve().parent
SIZES=[16,20,24,32,40,48,64,128,256]

def render(size, active=False):
    factor=4
    im=Image.new('RGBA',(size*factor,size*factor))
    d=ImageDraw.Draw(im)
    def box(values): return tuple(round(v*size*factor/256) for v in values)
    d.rounded_rectangle(box((8,8,248,248)),radius=round(60*size*factor/256),fill='#0d877d')
    # An oscilloscope trace, not a shield/certification symbol.
    points=[(40,137),(80,137),(102,79),(137,186),(164,115),(186,137),(215,137)]
    points=[box(point) for point in points]
    d.line(points,fill='white',width=max(3,round(17*size*factor/256)),joint='curve')
    for x,y in [points[0],points[-1]]:
        r=round(8.5*size*factor/256);d.ellipse((x-r,y-r,x+r,y+r),fill='white')
    if active:
        d.ellipse(box((175,7,253,85)),fill='#102c3c')
        d.ellipse(box((184,16,244,76)),fill='#ffc857')
    return im.resize((size,size),Image.Resampling.LANCZOS)

for name,active in [('superopti',False),('superopti-active',True)]:
    render(256,active).save(ROOT/f'{name}.ico',format='ICO',sizes=[(n,n) for n in SIZES],append_images=[render(n,active) for n in SIZES])
    render(512,active).save(ROOT/f'{name}.png')
ROOT.joinpath('superopti.svg').write_text('''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" role="img" aria-label="SuperOpti pulse icon"><rect x="8" y="8" width="240" height="240" rx="60" fill="#0d877d"/><path d="M40 137h40l22-58 35 107 27-71 22 22h29" fill="none" stroke="#fff" stroke-width="17" stroke-linecap="round" stroke-linejoin="round"/></svg>''')
