"""Compiles a Cg program with a shipped cg.dll (Cg runtime) under Unicorn, the way the
NVDemo viewers do (GasStation.exe 0x460130): cgCreateProgram(ctx, CG_SOURCE, source,
profile, entry, {"-I<shader dir>", 0}), then cgGetProgramString(CG_COMPILED_PROGRAM).

    python3 cgdll_emu.py CG.DLL PROFILE ENTRY SOURCE OUT

PROFILE is vp20, fp20, vp30 or fp30. Includes are read from the source's directory.
Exits non-zero with the compiler's listing on stderr when compilation fails.
"""
import os
import struct
import sys

import cgc_emu
cgc_emu.HEAP = 0x30000000  # cg.dll's image base is 0x10000000
from cgc_emu import Emu, STACK, STACK_SIZE, THUNKS  # noqa: E402
from unicorn import UcError  # noqa: E402
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP,  # noqa: E402
                               UC_X86_REG_FPCW)

PROFILES = {"vp20": 6146, "fp20": 6147, "vp30": 6148, "fp30": 6149}
CG_SOURCE, CG_COMPILED_PROGRAM = 4112, 4106
DONE = THUNKS + 0xFFF0


class Dll(Emu):
    def __init__(self, dll):
        super().__init__(dll, [])
        self.exports = {e.name.decode(): self.base + e.address
                        for e in self.pe.DIRECTORY_ENTRY_EXPORT.symbols if e.name}
        self.uc.reg_write(UC_X86_REG_FPCW, 0x027F)
        self.sp = STACK + STACK_SIZE - 0x100
        self.ok = True

    def call(self, address, *args):
        sp = self.sp - 4 * len(args) - 4
        self.uc.mem_write(sp, struct.pack(f"<{len(args) + 1}I", DONE, *args))
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        try:
            self.uc.emu_start(address, DONE)
        except UcError as e:
            raise RuntimeError(f"emulation error {e} at {self.uc.reg_read(UC_X86_REG_EIP):#x}")
        if self.exit_code is not None:
            raise RuntimeError(f"process exit {self.exit_code}")
        return self.uc.reg_read(UC_X86_REG_EAX)

    def string(self, s):
        p = self.alloc(len(s) + 1)
        self.uc.mem_write(p, s + b"\0")
        return p

    # What the DLL imports beyond cgc.exe.
    def k_OutputDebugStringA(self):
        sys.stderr.write(self.cstr(self.arg(0)).decode("latin1")); self.ret(0, 1)
    def k_DebugBreak(self): raise RuntimeError("DebugBreak")
    def k_SetEnvironmentVariableA(self): self.ret(1, 2)
    def k_CompareStringA(self):
        a, na, b, nb = self.arg(2), self.arg(3), self.arg(4), self.arg(5)
        sa = self.cstr(a) if na == 0xFFFFFFFF else bytes(self.uc.mem_read(a, na))
        sb = self.cstr(b) if nb == 0xFFFFFFFF else bytes(self.uc.mem_read(b, nb))
        if self.arg(1) & 1:
            sa, sb = sa.lower(), sb.lower()
        self.ret(1 if sa < sb else 3 if sa > sb else 2, 6)
    def k_CompareStringW(self): raise RuntimeError("CompareStringW")
    def k_RegQueryValueA(self): self.ret(2, 4)  # ERROR_FILE_NOT_FOUND
    def k_GetModuleHandleA(self): self.ret(self.base, 1)
    def k_GetModuleFileNameA(self):
        buf, n = self.arg(1), self.arg(2)
        name = b"C:\\cg.dll"
        self.uc.mem_write(buf, name[: n - 1] + b"\0"); self.ret(len(name), 3)
    def k_CreateFileA(self):
        # The compiler opens includes as "<dir>/<name>" with the -I directory.
        name = self.cstr(self.arg(0)).decode().replace("\\", "/")
        self.uc.mem_write(self.arg(0), name.encode() + b"\0")
        super().k_CreateFileA()


def main():
    dll, profile, entry, source, out = sys.argv[1:6]
    e = Dll(dll)
    entry_point = e.base + e.pe.OPTIONAL_HEADER.AddressOfEntryPoint
    if not e.call(entry_point, e.base, 1, 0):
        raise SystemExit("DllMain failed")
    ctx = e.call(e.exports["cgCreateContext"])
    # SOURCE as the scene names it, relative to the viewer's bin directory (the working
    # directory), e.g. "..\\shaders\\sky.vp30"; the viewer cuts it at the last slash.
    src = open(source.replace("\\", "/"), "rb").read()
    cut = max(source.rfind("/"), source.rfind("\\"))
    include = source[:cut] if cut >= 0 else "."
    arg = e.string(b"-I" + include.encode())
    args = e.alloc(8)
    e.uc.mem_write(args, struct.pack("<II", arg, 0))
    prog = e.call(e.exports["cgCreateProgram"], ctx, CG_SOURCE, e.string(src),
                  PROFILES[profile], e.string(entry.encode()), args)
    err = e.call(e.exports["cgGetError"])
    listing = e.call(e.exports["cgGetLastListing"], ctx)
    text = e.cstr(listing).decode("latin1") if listing else ""
    if err or not prog:
        sys.stderr.write(f"error {err}\n{text}")
        sys.exit(1)
    code = e.call(e.exports["cgGetProgramString"], prog, CG_COMPILED_PROGRAM)
    open(out, "wb").write(e.cstr(code))
    if text:
        sys.stderr.write(text)


if __name__ == "__main__":
    main()
