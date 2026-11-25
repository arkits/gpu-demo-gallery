"""Rename cab members using an unpacked MSI's File/Component/Directory tables."""
import sys,struct,os,shutil
d,cabd,out=sys.argv[1:4]
pool=open(d+'/0_StringPool','rb').read(); data=open(d+'/0_StringData','rb').read()
S=['']; o=0; i=4
while i<len(pool):
    l,r=struct.unpack_from('<HH',pool,i); i+=4
    if l==0 and r: l=struct.unpack_from('<I',pool,i)[0]; i+=4
    S.append(data[o:o+l].decode('latin1')); o+=l
sz=3 if struct.unpack_from('<I',pool,0)[0]&0x80000000 else 2
def col(t):
    c=open(d+'/0_Columns','rb').read(); n=len(c)//(2+2*sz+2) if 0 else len(c)//(sz+2+sz+2)
    g=lambda b,o,s:int.from_bytes(b[o:o+s],'little'); A=[0,sz*n,(sz+2)*n,(2*sz+2)*n]; R=[]
    for k in range(n):
        if S[g(c,A[0]+sz*k,sz)]==t: R.append((g(c,A[1]+2*k,2)&0x7fff,S[g(c,A[2]+sz*k,sz)],g(c,A[3]+2*k,2)&0x7fff))
    R.sort(); return R
def tab(t):
    b=open(d+'/0'+t,'rb').read(); C=col(t); W=[(sz if ty&0x800 else (4 if ty&0xff==4 else 2)) for _,_,ty in C]; n=len(b)//sum(W); o=0; V=[]
    for w,(_,nm,ty) in zip(W,C):
        v=[int.from_bytes(b[o+w*k:o+w*k+w],'little') for k in range(n)]; o+=w*n
        V.append([S[x] if ty&0x800 else x^(1<<(8*w-1)) for x in v])
    return [dict(zip([c[1] for c in C],r)) for r in zip(*V)]
D={r['Directory']:r for r in tab('Directory')}
def path(k):
    r=D.get(k)
    if not r: return ''
    n=r['DefaultDir'].split(':')[0].split('|')[-1]
    par=r['Directory_Parent']; return (os.path.join(path(par),n) if par and par!=k else '').replace('/.','')
C={r['Component']:r['Directory_'] for r in tab('Component')}
ok=0;miss=[];names=set(os.listdir(cabd)); tot=0
for f in tab('File'):
    p=os.path.join(path(C.get(f['Component_'],'')),f['FileName'].split('|')[-1]); src=os.path.join(cabd,f['File']); tot+=1
    if not os.path.exists(src): miss.append(f['File']); continue
    q=os.path.normpath(os.path.join(out,p.lstrip('/'))); assert q.startswith(os.path.abspath(out))
    os.makedirs(os.path.dirname(q),exist_ok=True); shutil.copy(src,q); ok+=1
print(ok,'of',tot,'missing',miss[:4],'cab members',len(names))
