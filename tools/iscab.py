"""List/extract InstallShield 5/6 data1.hdr + data1.cab (read-only, paths sanitized)."""
import struct,sys,zlib,os,hashlib
hdr=open(sys.argv[1],'rb').read(); cab=open(sys.argv[2],'rb').read(); out=sys.argv[3] if len(sys.argv)>3 else None
u32=lambda o:struct.unpack_from('<I',hdr,o)[0]; u16=lambda o:struct.unpack_from('<H',hdr,o)[0]
sig,ver,vol,doff,dsize=struct.unpack_from('<5I',hdr,0); assert sig==0x28635349
major=(ver>>12)&0xf if ver>>24==1 else 0
print('version',hex(ver),'major',major)
ft=doff+u32(doff+0xc); nd=u32(doff+0x1c); nf=u32(doff+0x28); fo2=u32(doff+0x2c)
def s(o): return hdr[o:hdr.index(b'\0',o)].decode('latin1')
tab=[u32(ft+4*i) for i in range(nd+(nf if major<6 else 0))]
dirs=[s(ft+tab[i]) for i in range(nd)]
bad=0;total=0
for i in range(nf):
    if major<6:
        p=ft+tab[nd+i]; no,di=u32(p),u32(p+4); fl=u16(p+8); ex,co=u32(p+10),u32(p+14); da=u32(p+0x26); md=hdr[p+0x2a:p+0x3a]
    else:
        p=ft+fo2+i*0x57; fl=u16(p); ex=u32(p+2); co=u32(p+10); da=u32(p+18); md=hdr[p+26:p+42]; no=u32(p+58); di=u16(p+62)
    if fl&8 or not no: continue
    n=s(ft+no); d=dirs[di] if di<len(dirs) else ''
    path=(d.replace('\\','/')+'/'+n).lstrip('/'); total+=ex
    raw=cab[da:da+co]; ok='?'
    try:
        if fl&4:
            o=0; b=b''
            while o+2<=len(raw) and len(b)<ex:
                z=struct.unpack_from('<H',raw,o)[0]; o+=2; b+=zlib.decompressobj(-15).decompress(raw[o:o+z]); o+=z
        else: b=raw[:ex]
        ok='ok' if len(b)==ex else 'SIZE %d'%len(b)
        if ok!='ok': bad+=1
    except Exception as e: ok='ERR '+str(e); bad+=1; b=None
    print(ex,hex(fl),path,ok)
    if out and b is not None:
        q=os.path.normpath(os.path.join(out,path)); assert q.startswith(os.path.abspath(out)+os.sep)
        os.makedirs(os.path.dirname(q),exist_ok=True); open(q,'wb').write(b)
print('files',nf,'bad',bad,'bytes',total)
