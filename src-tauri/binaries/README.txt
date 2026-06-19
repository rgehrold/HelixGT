Runtime binaries bundled with Universal Gene Tool.

Tracked in git (Windows):
  minimap2.exe
  samtools.exe
  *.dll                  (25 runtime libraries — not the full MSYS2 /mingw64/bin dump)

macOS/Linux builds would use unprefixed minimap2 and samtools binaries instead.

The DLLs here are the minimum set required to run both tools on a clean Windows
machine (no MSYS2 install). Extra libraries copied from /mingw64/bin during
setup — GnuTLS, Nettle, ncurses UI helpers, etc. — were removed; they are only
needed when building samtools from source, not at runtime.

These files are declared in tauri.conf.json > bundle > resources and install to
$RESOURCE/binaries/ in packaged apps. The app resolves those paths automatically.

During development the app also checks:
  1. src-tauri/binaries/ (this folder)
  2. binaries/ next to the running executable
  3. System PATH

Replacing binaries:
  minimap2: https://github.com/lh3/minimap2/releases
            (needs zlib1.dll and libwinpthread-1.dll on Windows)
  samtools: https://packages.msys2.org/package/mingw-w64-x86_64-samtools
            Copy samtools.exe plus required DLLs; trim with the same runtime test
            or copy only what a fresh MSYS2 samtools install pulls in.

Source trees (minimap2-*/, samtools-*/) are gitignored and not needed at runtime.