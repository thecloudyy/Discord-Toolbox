# -*- mode: python ; coding: utf-8 -*-

import os
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# curl_cffi ships its native libcurl DLL via delvewheel in a `curl_cffi.libs`
# directory next to the package, and libcurl in turn depends on the OpenSSL
# DLLs (libcrypto/libssl) from the Python install. PyInstaller does not detect
# these data DLLs on its own, so we collect them explicitly and bundle them
# alongside curl_cffi's own modules -- i.e. under `<MEIPASS>/curl_cffi/`,
# right next to `_wrapper.pyd`.
#
# Python's extension-module loader always searches a .pyd's own directory when
# resolving that .p'd dependencies (LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR), so
# bundling libcurl (and the OpenSSL DLLs it loads) there is what lets
# curl_cffi's C extension resolve libcurl at import time.
# ---------------------------------------------------------------------------
def _curl_binaries():
    binaries = []

    # curl_cffi's delvewheel-shipped libcurl DLL.
    try:
        import curl_cffi

        libs_dir = Path(curl_cffi.__file__).resolve().parent.parent / "curl_cffi.libs"
        if libs_dir.is_dir():
            for dll in libs_dir.glob("*.dll"):
                binaries.append((str(dll), "curl_cffi"))
    except Exception:
        pass

    # OpenSSL/libcurl transitive dependencies from the base Python install.
    base_dlls = Path(getattr(sys, "base_prefix", sys.prefix)) / "DLLs"
    for name in ("libcrypto-3.dll", "libssl-3.dll", "libffi-8.dll", "zlib1.dll"):
        dll = base_dlls / name
        if dll.is_file():
            binaries.append((str(dll), "curl_cffi"))

    # _wrapper.pyd also imports python3t.dll (a curl_cffi build artifact). It
    # lives under the base install's abi3t-compat directory, which PyInstaller
    # does not scan, so bundle it explicitly next to _wrapper.pyd.
    base_prefix = Path(getattr(sys, "base_prefix", sys.prefix))
    for python3t in list(base_prefix.rglob("python3t.dll"))[:1]:
        binaries.append((str(python3t), "curl_cffi"))

    return binaries


a = Analysis(
    ['src/run_bot.py'],
    pathex=['src'],
    binaries=_curl_binaries(),
    datas=[],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name='discord-toolbox-bot',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
