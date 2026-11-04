#!/usr/bin/env python3
"""Compare two identically framed PNG captures; Pillow is optional (no auto-install).
This measures pixel differences, not gameplay parity. Output metrics and optional heatmap.
"""
import argparse
import json
import math
import sys
from pathlib import Path


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('original');p.add_argument('port')
    p.add_argument('--max-mae',type=float,default=0.0,help='allowed mean per-channel absolute error (0..255)')
    p.add_argument('--max-pixel-error',type=int,default=0,help='allowed max per-channel error (0..255)')
    p.add_argument('--heatmap',help='save absolute RGB difference as PNG')
    p.add_argument('--report',help='save JSON metrics')
    a=p.parse_args()
    try:
        from PIL import Image, ImageChops, ImageStat
    except ImportError:
        print('Missing optional Pillow dependency: install it explicitly (python3 -m pip install Pillow)',file=sys.stderr)
        return 2
    if not (0 <= a.max_mae <= 255 and 0 <= a.max_pixel_error <= 255):
        print('Thresholds must be within 0..255',file=sys.stderr);return 2
    with Image.open(a.original) as o, Image.open(a.port) as r:
        if o.size!=r.size:
            result={'pass':False,'reason':'dimensions differ','original_size':o.size,'port_size':r.size}
        else:
            # RGBA is important: masks/transparent pixels must not silently pass.
            aa=o.convert('RGBA'); bb=r.convert('RGBA')
            difference=ImageChops.difference(aa,bb)
            st=ImageStat.Stat(difference)
            mae=sum(st.mean)/4.0
            rms=math.sqrt(sum(v*v for v in st.rms)/4.0)
            bounds=difference.getextrema()
            max_error=max(high for low,high in bounds)
            result={'pass':mae<=a.max_mae and max_error<=a.max_pixel_error,
                'width':o.width,'height':o.height,'mae':mae,'rmse':rms,
                'max_channel_error':max_error,'threshold_mae':a.max_mae,
                'threshold_max_error':a.max_pixel_error,
                'note':'RGBA per-channel pixel comparison; only valid with matched camera, time, assets and rendering settings.'}
            if a.heatmap:
                dst=Path(a.heatmap);dst.parent.mkdir(parents=True,exist_ok=True)
                difference.convert('RGB').save(dst)
                result['heatmap']=str(dst)
    if a.report:
        dst=Path(a.report);dst.parent.mkdir(parents=True,exist_ok=True)
        dst.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
    return 0 if result['pass'] else 1

if __name__=='__main__':sys.exit(main())
