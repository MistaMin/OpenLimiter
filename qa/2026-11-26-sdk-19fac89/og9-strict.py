import json,re,sys,os
d0=sys.argv[1]
for fmt in ['VST3','AU']:
    pf=os.path.join(d0,f'OpenLimiter_{fmt}_pattern.json'); lf=os.path.join(d0,f'OpenLimiter_{fmt}_load.json')
    if not (os.path.exists(pf) and os.path.exists(lf)): continue
    pat={}
    for l in open(pf):
        m=re.match(r'^(\d+)=([^:]+):(.+)$',l.strip())
        if m: pat[int(m[1])]=(float(m[2]),float(m[3]))
    d=json.load(open(lf)); rv=d.get('read_values') or []; lab=d.get('labels') or []
    bad=[(i,lab[i] if i<len(lab) else '?',round(r,4),rv[i]) for i,(v,r) in pat.items() if i<len(rv) and rv[i] is not None and abs(rv[i]-r)>0.01]
    print(d0.split('/qa/')[-1], fmt, 'strict mismatches', len(bad), bad[:6])
