import os,struct,collections,re,datetime
S=os.environ.get('UNPACKED','.').rstrip('/')+'/'
def imports(p):
    b=open(p,'rb').read()
    try:
        if b[:2]!=b'MZ': return None
        pe=struct.unpack_from('<I',b,0x3c)[0]
        if b[pe:pe+4]!=b'PE\0\0': return None
        n=struct.unpack_from('<H',b,pe+6)[0]; ts=struct.unpack_from('<I',b,pe+8)[0]; m=struct.unpack_from('<H',b,pe+4)[0]; os_=struct.unpack_from('<H',b,pe+20)[0]
        mag=struct.unpack_from('<H',b,pe+24)[0]; dd=pe+24+(96 if mag==0x10b else 112); ir=struct.unpack_from('<I',b,dd+8)[0]
        dbg=struct.unpack_from('<I',b,dd+48)[0]
        sec=[]
        for i in range(n):
            o=pe+24+os_+40*i; sec.append((b[o:o+8].rstrip(b'\0').decode('latin1'),)+struct.unpack_from('<4I',b,o+8))
        def off(r):
            for nm,vs,va,rs,rp in sec:
                if va<=r<va+max(vs,rs): return r-va+rp
        L=[];o=off(ir) if ir else None
        while o:
            e=struct.unpack_from('<5I',b,o)
            if not e[3]: break
            q=off(e[3]); L.append(b[q:b.index(b'\0',q)].decode('latin1').lower()); o+=20
        pdb=re.search(rb'[A-Za-z]:\\[^\0]{3,120}\.pdb',b)
        return (datetime.datetime.utcfromtimestamp(ts).strftime('%Y-%m-%d'),{0x14c:'x86',0x8664:'x64'}.get(m,hex(m)),L,len(b),pdb.group().decode('latin1') if pdb else '',[s[0] for s in sec], struct.unpack_from('<I',b,pe+12)[0])
    except Exception as e: return ('err',str(e),[],len(b),'',[],0)
skip=re.compile(r'vcredist|dxsetup|dsetup|instmsi|_inst32|uninst|_isdel|/setup\.exe|ikernel|modern-|nsdialogs|dxupdate|D3D11Install',re.I)
sys_=re.compile(r'^(kernel32|user32|gdi32|advapi32|shell32|ole32|oleaut32|msvc.*|comctl32|comdlg32|winmm|version|ws2_32|wsock32|imm32|mfc.*|shlwapi|rpcrt4)\.',0)
import sys
bases=[]
for n in sorted(set(os.listdir(S+'x'))|set(os.listdir(S+'y'))):
    y=S+'y/'+n
    bases.append((n,y if os.path.isdir(y) and any(f for _,_,f in os.walk(y)) else S+'x/'+n))
for n,b in bases:
    if sys.argv[1:] and not any(a in n for a in sys.argv[1:]): continue
    c=collections.Counter(); sz=collections.Counter(); ex=[]; tot=0; rd=[]
    for d,_,fs in os.walk(b):
        if '/TEMP' in d or '/_TEMP' in d: continue
        for f in fs:
            p=os.path.join(d,f); s=os.path.getsize(p); e=f.rsplit('.',1)[-1].lower() if '.' in f else '-'
            c[e]+=1; sz[e]+=s; tot+=s
            if e in('exe','scr','dll') and not skip.search(p):
                r=imports(p)
                if r: ex.append((p[len(b)+1:],r))
            if re.search(r'read ?me|\.txt$',f,re.I) and s<60000: rd.append(p[len(b)+1:])
    print(f'\n## {n}  {tot/1e6:.1f} MB {sum(c.values())} files')
    print('  ',', '.join(f'{e}:{c[e]}({sz[e]/1e6:.1f})' for e,_ in sz.most_common(16)))
    for p,r in ex:
        if p.lower().endswith('.dll') and r[3]<200000: continue
        if re.search(r'cg(GL|D3D\d*)?\.dll|glut|mfc|msvc|d3dx|fmod|physx|optix|devil|ilu|mss32|openexr|half|fw1|zlib|d3dcomp|aturl|isscript|ctor|isrt|objps|corecomp|iscript|_isres|dotnet|iuser',p,re.I): continue
        print('   EXE',p,r[0],r[1],f'{r[3]/1e3:.0f}K',[x[:-4] for x in r[2] if not sys_.match(x)],r[4][-60:],'' if '.text' in r[5] else r[5])
