"""InstallShield 3 archive (PKWARE DCL implode)."""
import sys,struct,os
b=open(sys.argv[1],'rb').read(); out=os.path.abspath(sys.argv[2])
print(b[:4].hex(),len(b))
toc=struct.unpack_from('<I',b,41)[0]; nd=struct.unpack_from('<H',b,49)[0]; 
nf=struct.unpack_from('<H',b,12)[0]; print('files',nf,'toc',hex(toc),'dirs',nd)
def blast(s):
    L=[]
    def tbl(rep):
        ls=[]
        for x in rep: ls+= [x&15]*((x>>4)+1)
        cnt=[0]*14
        for l in ls: cnt[l]+=1
        off=[0]*15
        for i in range(1,13): off[i+1]=off[i]+cnt[i]
        sym=[0]*len(ls)
        for i,l in enumerate(ls):
            if l: sym[off[l]]=i; off[l]+=1
        return cnt,sym
    lit=tbl([11,124,8,7,28,7,188,13,76,4,10,8,12,10,12,10,8,23,8,9,7,6,7,8,7,6,55,8,23,24,12,11,7,9,11,12,6,7,22,5,7,24,6,11,9,6,7,22,7,11,38,7,9,8,25,11,8,11,9,12,8,12,5,38,5,38,5,11,7,5,6,21,6,10,53,8,7,24,10,27,44,253,253,253,252,252,252,13,12,45,12,45,12,61,12,45,44,173])
    ln=tbl([2,35,36,53,38,23]); ds=tbl([2,20,53,230,247,151,248])
    base=[3,2,4,5,6,7,8,9,10,12,16,24,40,72,136,264]; ext=[0]*8+[1,2,3,4,5,6,7,8]
    pos=2; bb=0; bc=0; n=len(s); lf,dic=s[0],s[1]; o=bytearray()
    def bits(k):
        nonlocal pos,bb,bc
        while bc<k: bb|=s[pos]<<bc; pos+=1; bc+=8
        v=bb&((1<<k)-1); bb>>=k; bc-=k; return v
    def dec(t):
        cnt,sym=t; code=first=index=0
        for l in range(1,14):
            code|=bits(1)^1; c=cnt[l]
            if code<first+c: return sym[index+code-first]
            index+=c; first=(first+c)<<1; code<<=1
        raise ValueError
    while True:
        if bits(1):
            y=dec(ln); l=base[y]+bits(ext[y])
            if l==519: break
            sh=2 if l==2 else dic
            dist=(dec(ds)<<sh)+bits(sh)+1
            assert dist<=len(o)
            for _ in range(l): o.append(o[-dist])
        else: o.append(dec(lit) if lf else bits(8))
    return bytes(o)
p=toc; dirs=[]
for i in range(nd):
    fc,cs,nl=struct.unpack_from('<HHH',b,p); dirs.append((fc,b[p+6:p+6+nl].decode('latin1'))); p+=cs
print(dirs)
R=[]
for fc,dn in dirs:
    for i in range(fc):
        e=b[p:]if 0 else None
        ex,co,of=struct.unpack_from('<III',b,p+3); cs=struct.unpack_from('<H',b,p+23)[0]; nl=b[p+29]; nm=b[p+30:p+30+nl].decode('latin1'); p+=cs
        R.append((dn,nm,ex,co,of))
if len(sys.argv)>3: 
    for r in R: print(r)
import multiprocessing as mp
def go(r):
    dn,nm,ex,co,of=r
    try:
        d=blast(b[of:of+co]); assert len(d)==ex,(len(d),ex)
        q=os.path.normpath(os.path.join(out,dn.replace('\\','/').strip('/'),nm)); assert q.startswith(out+os.sep); os.makedirs(os.path.dirname(q),exist_ok=True); open(q,'wb').write(d); return 1
    except Exception as e: return (nm,repr(e)[:80])
if __name__=='__main__':
    mp.set_start_method('fork'); 
    with mp.Pool(8) as P: r=P.map(go,R,chunksize=1)
    print('ok',r.count(1),'of',len(R),[x for x in r if x!=1][:4])
