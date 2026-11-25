"""Split InstallShield setup launchers (plain and encoded overlay)."""
import sys,struct,os
p,out=sys.argv[1:3]; b=open(p,'rb').read(); pe=struct.unpack_from('<I',b,0x3c)[0]
n=struct.unpack_from('<H',b,pe+6)[0]; o=pe+24+struct.unpack_from('<H',b,pe+20)[0]; end=0
for i in range(n):
    vs,va,rs,rp=struct.unpack_from('<4I',b,o+40*i+8); end=max(end,rs+rp)
def put(name,d):
    name=name.replace('\\','/').lstrip('/'); assert '..' not in name
    q=os.path.join(out,name); os.makedirs(os.path.dirname(q),exist_ok=True); open(q,'wb').write(d); print(name,len(d),d[:8])
if b[end:end+13]==b'InstallShield':
    cnt,=struct.unpack_from('<H',b,end+14); p=end+46
    for k in range(cnt):
        name=b[p:p+260].split(b'\0')[0]; fl,x,ln=struct.unpack_from('<III',b,p+260); p+=312
        d=bytearray(b[p:p+ln]); p+=ln
        if fl&6:
            magic=bytes([0x13,0x35,0x86,0x07]); key=bytes(c^magic[i%4] for i,c in enumerate(name)); L=len(key)
            for i in range(len(d)):
                j=i%1024 if fl&4 else i
                c=d[i]; d[i]=(~(key[j%L]^(((c<<4)|(c>>4))&255)))&255
        put(name.decode('latin1'),bytes(d))
else:
    p=end
    if b[p:p+1].isdigit() is False: pass
    while p<len(b)-4:
        f=[]
        q=p
        for _ in range(4):
            e=b.find(b'\0',q)
            if e<0 or e-q>300: f=None;break
            f.append(b[q:e].decode('latin1')); q=e+1
        if not f or not f[3].isdigit(): print('stop',hex(p),len(b)); break
        ln=int(f[3]); put(f[1] or f[0],b[q:q+ln]); p=q+ln
