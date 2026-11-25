"""Extract InstallShield 5-7 cabinets, multi-volume (read-only, sanitized, MD5 checked)."""
import struct,sys,zlib,os,hashlib
d,out=sys.argv[1],os.path.abspath(sys.argv[2]); hdr=open(d+'/data1.hdr','rb').read(); cabs={}
def cab(v):
    if v not in cabs: cabs[v]=open(f'{d}/data{v}.cab','rb').read()
    return cabs[v]
u32=lambda o:struct.unpack_from('<I',hdr,o)[0]; u16=lambda o:struct.unpack_from('<H',hdr,o)[0]
ver=u32(4); major=(ver>>12)&0xf if ver>>24==1 else 0; doff=u32(12)
ft=doff+u32(doff+0xc); nd=u32(doff+0x1c); nf=u32(doff+0x28); fo2=u32(doff+0x2c)
s=lambda o:hdr[o:hdr.index(b'\0',o)].decode('latin1')
tab=[u32(ft+4*i) for i in range(nd+(nf if major<6 else 0))]; dirs=[s(ft+tab[i]) for i in range(nd)]
R=[]
for i in range(nf):
    if major<6:
        p=ft+tab[nd+i]; R.append(dict(no=u32(p),di=u32(p+4),fl=u16(p+8),ex=u32(p+10),co=u32(p+14),da=u32(p+0x26),md=hdr[p+0x2a:p+0x3a],v=1,prev=0,lf=0))
    else:
        p=ft+fo2+i*0x57; R.append(dict(fl=u16(p),ex=u32(p+2),co=u32(p+10),da=u32(p+18),md=hdr[p+26:p+42],no=u32(p+58),di=u16(p+62),prev=u32(p+76),lf=hdr[p+84],v=u16(p+85)))
ok=bad=dup=md=tot=0
for i,f in enumerate(R):
    if f['fl']&8 or not f['no']: continue
    n=s(ft+f['no']); dd=dirs[f['di']] if f['di']<len(dirs) else ''
    g=f; k=0
    while g['lf']&1 and k<50: g=R[g['prev']]; k+=1
    try:
        raw=cab(g['v'])[g['da']:g['da']+g['co']]
        if g['fl']&2: raw=bytes((((b^0xd5)>>2)|((b^0xd5)<<6)&255)-(j%0x47)&255 for j,b in enumerate(raw))
        if g['fl']&4:
            o=0;b=b''; L=[]; t=0
            while o+2<=len(raw) and t<g['ex']:
                z=struct.unpack_from('<H',raw,o)[0]; o+=2; x=zlib.decompressobj(-15).decompress(raw[o:o+z]); L.append(x); t+=len(x); o+=z
            b=b''.join(L)
        else: b=raw[:g['ex']]
        assert len(b)==g['ex'],(len(b),g['ex'])
        ok+=1; tot+=len(b)
        if hashlib.md5(b).digest()==g['md']: md+=1
        q=os.path.normpath(os.path.join(out,dd.replace('\\','/').strip('/'),n.replace('/','_')))
        assert q.startswith(out+os.sep)
        if os.path.exists(q):
            if open(q,'rb').read()!=b: q+='.alt%d'%i; dup+=1
        os.makedirs(os.path.dirname(q),exist_ok=True); open(q,'wb').write(b)
    except Exception as e: bad+=1; print('  BAD',dd,n,repr(e)[:80],g['v'],hex(g['fl'])) if bad<4 else 0
print(f'major {major} files {nf} ok {ok} md5ok {md} bad {bad} conflicts {dup} bytes {tot}')
