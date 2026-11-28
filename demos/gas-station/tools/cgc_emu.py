"""Runs NVIDIA's cgc.exe (Win32 console, static CRT) under Unicorn with a tiny KERNEL32.

    python3 cgc_emu.py CGC.EXE [cgc arguments...]

Files named in the arguments are read from and written to the host. stdout and stderr go to
the host's. Only what this cgc.exe imports is provided.
"""
import struct
import sys

import pefile
from unicorn import (UC_ARCH_X86, UC_HOOK_CODE, UC_HOOK_INTR, UC_HOOK_MEM_UNMAPPED,
                     UC_MODE_32, Uc, UcError)
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX,
                               UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_FS,
                               UC_X86_REG_GDTR, UC_X86_REG_DS, UC_X86_REG_ES, UC_X86_REG_SS,
                               UC_X86_REG_CS, UC_X86_REG_FPCW)

STACK, STACK_SIZE = 0x00200000, 0x00100000
HEAP, HEAP_SIZE = 0x10000000, 0x08000000
THUNKS = 0x0F000000
TEB = 0x0E000000
GDT = 0x0E100000
STRINGS = 0x0E200000


class Emu:
    def __init__(self, exe, args):
        self.pe = pefile.PE(exe)
        self.base = self.pe.OPTIONAL_HEADER.ImageBase
        self.uc = Uc(UC_ARCH_X86, UC_MODE_32)
        size = (self.pe.OPTIONAL_HEADER.SizeOfImage + 0xFFF) & ~0xFFF
        self.uc.mem_map(self.base, size)
        self.uc.mem_write(self.base, self.pe.header)
        for s in self.pe.sections:
            data = s.get_data()
            self.uc.mem_write(self.base + s.VirtualAddress, data[: s.Misc_VirtualSize or len(data)])
        for addr, length in [(STACK, STACK_SIZE), (HEAP, HEAP_SIZE), (THUNKS, 0x10000),
                             (TEB, 0x10000), (GDT, 0x1000), (STRINGS, 0x10000)]:
            self.uc.mem_map(addr, length)
        self.heap_top = HEAP
        self.sizes = {}
        self.files = {1: sys.stdin.buffer, 2: sys.stdout.buffer, 3: sys.stderr.buffer}
        self.next_handle = 0x100
        self.exit_code = None
        self.command_line = " ".join(['cgc'] + args).encode() + b"\0"
        self.uc.mem_write(STRINGS, self.command_line)
        self.uc.mem_write(STRINGS + 0x1000, b"\0\0\0\0")
        self.uc.mem_write(STRINGS + 0x2000, b"C:\\cgc.exe\0")
        self.setup_fs()
        self.thunks = {}
        self.patch_imports()

    def setup_fs(self):
        # TEB: exception list at 0, stack base/limit, self pointer at 0x18.
        self.uc.mem_write(TEB, struct.pack("<IIIIIII", 0xFFFFFFFF, STACK + STACK_SIZE, STACK,
                                           0, 0, 0, TEB))
        def entry(base, limit, access, flags):
            return struct.pack("<HHBBBB", limit & 0xFFFF, base & 0xFFFF, (base >> 16) & 0xFF,
                               access, ((limit >> 16) & 0xF) | (flags << 4), (base >> 24) & 0xFF)
        gdt = bytearray(8 * 8)
        gdt[8:16] = entry(0, 0xFFFFF, 0x9B, 0xC)      # code
        gdt[16:24] = entry(0, 0xFFFFF, 0x93, 0xC)     # data
        gdt[24:32] = entry(TEB, 0xFFF, 0x93, 0x4)     # fs
        gdt[32:40] = entry(0, 0xFFFFF, 0x97 & ~0x60 | 0x00, 0xC)  # stack (ring 0 data)
        self.uc.mem_write(GDT, bytes(gdt))
        self.uc.reg_write(UC_X86_REG_GDTR, (0, GDT, len(gdt) - 1, 0))
        for reg in (UC_X86_REG_SS, UC_X86_REG_DS, UC_X86_REG_ES):
            self.uc.reg_write(reg, 0x10)
        self.uc.reg_write(UC_X86_REG_FS, 0x18)

    def patch_imports(self):
        at = THUNKS
        for entry in self.pe.DIRECTORY_ENTRY_IMPORT:
            for imp in entry.imports:
                name = imp.name.decode()
                self.uc.mem_write(at, b"\xc3")  # stand-in; the hook does the work
                self.thunks[at] = name
                self.uc.mem_write(imp.address, struct.pack("<I", at))
                at += 16
        self.uc.hook_add(UC_HOOK_CODE, self.on_thunk, begin=THUNKS, end=THUNKS + 0x10000)

    # Memory helpers.
    def u32(self, a):
        return struct.unpack("<I", self.uc.mem_read(a, 4))[0]

    def arg(self, i):
        return self.u32(self.uc.reg_read(UC_X86_REG_ESP) + 4 + 4 * i)

    def cstr(self, a):
        out = bytearray()
        while True:
            c = self.uc.mem_read(a, 1)[0]
            if c == 0:
                return bytes(out)
            out.append(c)
            a += 1

    def alloc(self, n):
        n = max(n, 1)
        p = self.heap_top
        self.heap_top += (n + 15) & ~15
        if self.heap_top > HEAP + HEAP_SIZE:
            raise MemoryError("heap exhausted")
        self.sizes[p] = n
        return p

    def ret(self, value, argc):
        esp = self.uc.reg_read(UC_X86_REG_ESP)
        back = self.u32(esp)
        self.uc.reg_write(UC_X86_REG_EAX, value & 0xFFFFFFFF)
        self.uc.reg_write(UC_X86_REG_ESP, esp + 4 + 4 * argc)
        self.uc.reg_write(UC_X86_REG_EIP, back)

    def on_thunk(self, uc, address, size, data):
        name = self.thunks.get(address)
        if name is None:
            return
        f = getattr(self, "k_" + name, None)
        if f is None:
            raise RuntimeError(f"unimplemented import {name}")
        f()

    # KERNEL32, by what the CRT needs.
    def k_GetVersion(self): self.ret(0x0A280105, 0)
    def k_GetCommandLineA(self): self.ret(STRINGS, 0)
    def k_GetStartupInfoA(self):
        self.uc.mem_write(self.arg(0), struct.pack("<I", 68) + bytes(64)); self.ret(0, 1)
    def k_GetStdHandle(self):
        n = self.arg(0) & 0xFFFFFFFF
        self.ret({0xFFFFFFF6: 1, 0xFFFFFFF5: 2, 0xFFFFFFF4: 3}.get(n, 0xFFFFFFFF), 1)
    def k_GetFileType(self): self.ret(2 if self.arg(0) in (1, 2, 3) else 1, 1)
    def k_SetHandleCount(self): self.ret(self.arg(0), 1)
    def k_GetLastError(self): self.ret(0, 0)
    def k_HeapCreate(self): self.ret(0x1234, 3)
    def k_HeapDestroy(self): self.ret(1, 1)
    def k_HeapAlloc(self):
        n, flags = self.arg(2), self.arg(1)
        p = self.alloc(n)
        if flags & 8:
            self.uc.mem_write(p, bytes(n))
        self.ret(p, 3)
    def k_HeapFree(self): self.ret(1, 3)
    def k_HeapReAlloc(self):
        old, n, flags = self.arg(2), self.arg(3), self.arg(1)
        p = self.alloc(n)
        if flags & 8:
            self.uc.mem_write(p, bytes(n))
        keep = min(self.sizes.get(old, 0), n)
        if keep:
            self.uc.mem_write(p, bytes(self.uc.mem_read(old, keep)))
        self.ret(p, 4)
    def k_VirtualAlloc(self):
        n = (self.arg(1) + 0xFFF) & ~0xFFF
        p = self.alloc(n + 0x1000)
        p = (p + 0xFFF) & ~0xFFF
        self.uc.mem_write(p, bytes(n))
        self.ret(p, 4)
    def k_VirtualFree(self): self.ret(1, 3)
    def k_IsBadWritePtr(self): self.ret(0, 2)
    def k_GetModuleFileNameA(self):
        buf, n = self.arg(1), self.arg(2)
        name = b"C:\\cgc.exe"
        self.uc.mem_write(buf, name[: n - 1] + b"\0"); self.ret(len(name), 3)
    def k_GetModuleHandleA(self): self.ret(self.base if self.arg(0) == 0 else 0, 1)
    def k_GetProcAddress(self): self.ret(0, 2)
    def k_LoadLibraryA(self): self.ret(0, 1)
    def k_GetEnvironmentStrings(self): self.ret(STRINGS + 0x1000, 0)
    def k_GetEnvironmentStringsW(self): self.ret(STRINGS + 0x1000, 0)
    def k_FreeEnvironmentStringsA(self): self.ret(1, 1)
    def k_FreeEnvironmentStringsW(self): self.ret(1, 1)
    def k_GetACP(self): self.ret(1252, 0)
    def k_GetOEMCP(self): self.ret(437, 0)
    def k_GetCPInfo(self):
        self.uc.mem_write(self.arg(1), struct.pack("<I", 1) + b"?\0" + bytes(12)); self.ret(1, 2)
    def k_MultiByteToWideChar(self):
        src, n, dst, dn = self.arg(2), self.arg(3) & 0xFFFFFFFF, self.arg(4), self.arg(5)
        s = self.cstr(src) + b"\0" if n == 0xFFFFFFFF else bytes(self.uc.mem_read(src, n))
        if dn:
            self.uc.mem_write(dst, b"".join(bytes([c, 0]) for c in s[:dn]))
        self.ret(len(s), 6)
    def k_WideCharToMultiByte(self):
        src, n, dst, dn = self.arg(2), self.arg(3) & 0xFFFFFFFF, self.arg(4), self.arg(5)
        if n == 0xFFFFFFFF:
            w, a = [], src
            while True:
                c = struct.unpack("<H", self.uc.mem_read(a, 2))[0]; w.append(c); a += 2
                if c == 0: break
        else:
            w = list(struct.unpack(f"<{n}H", self.uc.mem_read(src, 2 * n)))
        s = bytes(c & 0xFF for c in w)
        if dn:
            self.uc.mem_write(dst, s[:dn])
        self.ret(len(s), 8)
    def k_GetStringTypeA(self):
        src, n, out = self.arg(2), self.arg(3), self.arg(4)
        self.k_types(src, n, out, 1); self.ret(1, 5)
    def k_GetStringTypeW(self):
        src, n, out = self.arg(1), self.arg(2), self.arg(3)
        self.k_types(src, n, out, 2); self.ret(1, 4)
    def k_types(self, src, n, out, width):
        res = []
        for i in range(n):
            c = self.uc.mem_read(src + i * width, 1)[0]
            t = 0
            if 65 <= c <= 90: t |= 0x1 | 0x100
            if 97 <= c <= 122: t |= 0x2 | 0x100
            if 48 <= c <= 57: t |= 0x4
            if c in b" \t\n\r\x0b\x0c": t |= 0x8
            if 33 <= c <= 47 or 58 <= c <= 64 or 91 <= c <= 96 or 123 <= c <= 126: t |= 0x10
            if c < 32 or c == 127: t |= 0x20
            if c == 32 or c == 9: t |= 0x40
            if 48 <= c <= 57 or 65 <= c <= 70 or 97 <= c <= 102: t |= 0x80
            res.append(t)
        self.uc.mem_write(out, struct.pack(f"<{n}H", *res))
    def k_LCMapStringA(self):
        src, n, dst, dn = self.arg(2), self.arg(3), self.arg(4), self.arg(5)
        s = bytes(self.uc.mem_read(src, n))
        if dn: self.uc.mem_write(dst, s[:dn])
        self.ret(len(s), 6)
    def k_LCMapStringW(self):
        src, n, dst, dn = self.arg(2), self.arg(3), self.arg(4), self.arg(5)
        s = bytes(self.uc.mem_read(src, 2 * n))
        if dn: self.uc.mem_write(dst, s[: 2 * dn])
        self.ret(n, 6)
    def k_CreateFileA(self):
        name = self.cstr(self.arg(0)).decode()
        access, disposition = self.arg(1), self.arg(4)
        try:
            if access & 0x40000000:
                f = open(name, "wb")
            else:
                f = open(name, "rb")
        except OSError:
            self.ret(0xFFFFFFFF, 7); return
        h = self.next_handle; self.next_handle += 4
        self.files[h] = f
        self.ret(h, 7)
    def k_ReadFile(self):
        h, buf, n, got = self.arg(0), self.arg(1), self.arg(2), self.arg(3)
        f = self.files[h]
        data = f.read(n) if h != 1 else b""
        self.uc.mem_write(buf, data)
        if got: self.uc.mem_write(got, struct.pack("<I", len(data)))
        self.ret(1, 5)
    def k_WriteFile(self):
        h, buf, n, done = self.arg(0), self.arg(1), self.arg(2), self.arg(3)
        f = self.files[h]
        f.write(bytes(self.uc.mem_read(buf, n)))
        if done: self.uc.mem_write(done, struct.pack("<I", n))
        self.ret(1, 5)
    def k_SetFilePointer(self):
        h, dist, how = self.arg(0), self.arg(1), self.arg(3)
        f = self.files[h]
        d = dist - (1 << 32) if dist & 0x80000000 else dist
        try:
            f.seek(d, how); pos = f.tell()
        except (OSError, ValueError):
            pos = 0
        self.ret(pos, 4)
    def k_CloseHandle(self):
        h = self.arg(0)
        f = self.files.pop(h, None)
        if f and h > 3: f.close()
        self.ret(1, 1)
    def k_FlushFileBuffers(self):
        f = self.files.get(self.arg(0))
        if f: f.flush()
        self.ret(1, 1)
    def k_SetEndOfFile(self): self.ret(1, 1)
    def k_SetStdHandle(self): self.ret(1, 2)
    def k_RtlUnwind(self): self.ret(0, 4)
    def k_RaiseException(self):
        esp = self.uc.reg_read(UC_X86_REG_ESP)
        raise RuntimeError(f"RaiseException code {self.arg(0):#x} flags {self.arg(1):#x} n {self.arg(2)} from {self.u32(esp):#x}")
    def k_UnhandledExceptionFilter(self): self.ret(1, 1)
    def k_GetCurrentProcess(self): self.ret(0xFFFFFFFF, 0)
    def k_TerminateProcess(self):
        self.exit_code = self.arg(1); self.uc.emu_stop()
    def k_ExitProcess(self):
        self.exit_code = self.arg(0); self.uc.emu_stop()

    def run(self):
        entry = self.base + self.pe.OPTIONAL_HEADER.AddressOfEntryPoint
        sp = STACK + STACK_SIZE - 0x100
        self.uc.mem_write(sp, struct.pack("<I", THUNKS + 0xFFF0))
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        # Windows starts threads with every x87 exception masked and 53-bit precision.
        self.uc.reg_write(UC_X86_REG_FPCW, 0x027F)

        def unmapped(uc, access, address, size, value, data):
            eip = uc.reg_read(UC_X86_REG_EIP)
            print(f"unmapped {address:#x} at {eip:#x}", file=sys.stderr)
            return False
        self.uc.hook_add(UC_HOOK_MEM_UNMAPPED, unmapped)
        try:
            self.uc.emu_start(entry, THUNKS + 0xFFF0)
        except UcError as e:
            print(f"emulation error {e} at {self.uc.reg_read(UC_X86_REG_EIP):#x}", file=sys.stderr)
            return 99
        for f in self.files.values():
            try: f.flush()
            except Exception: pass
        return self.exit_code or 0


if __name__ == "__main__":
    sys.exit(Emu(sys.argv[1], sys.argv[2:]).run())
