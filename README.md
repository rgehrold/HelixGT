# HelixGT

A native Windows desktop genomics workbench for visual locus browsing, file analysis, format conversion, merging, and read alignment. Built with **Rust**, **Tauri**, **Svelte**, and **noodles**, with **minimap2** and **samtools** for alignment and CRAM support.

## Features

### Convert

Batch-convert between common genomics formats:

- **Sequence:** FASTA, FASTQ, GenBank
- **Annotation:** GFF, BED
- **Alignment:** SAM, BAM, CRAM
- **Variants:** VCF

Examples: FASTA ↔ FASTQ, SAM ↔ BAM, CRAM → SAM/BAM, GenBank → FASTA. Text outputs can be gzip-compressed. CRAM input or output requires a reference FASTA.

### Merge

Combine multiple files of the same type into one output. Supported formats include FASTA, FASTQ, SAM, BAM, CRAM, GFF, BED, VCF, and GenBank. The app suggests a filename from common prefixes/suffixes in the input names.

### View

Browse sequence, annotation, and alignment files on a linear track (IGV-style):

- Open **FASTA** / **FASTQ** for reference bases
- Overlay **GFF** / **BED** feature tracks
- Open indexed **BAM** / **CRAM** for **coverage** and **individual reads**
  - Scrollable **read list** (one row per read) with filter and full-field inspector
  - Click coverage bars for depth/bin stats; click reads for CIGAR, flags, qualities, sequence
  - Pileup bars with CIGAR (matches, indels, soft-clips), strand/MAPQ/pair coloring
  - Mismatch highlighting vs open reference when zoomed to bases
  - Filters: secondary, supplementary, duplicates, min MAPQ
- Navigation: fit contig / fit selection, pan, zoom
- Copy a selected sequence region; reverse-complement reference display

Right-click a supported file → **Open in View**, or use the **View** mode tab.

Coverage reports mean depth per bin, including fractional overlaps and excluding introns. CRAM uses bundled samtools for indexed regional queries. Whole-contig depth is opt-in under **Display → Whole-contig depth overview** and loads independently of local tracks. The overview and windows wider than 2 Mb may be scan-limited estimates.

BAM/CRAM must be coordinate-sorted and indexed (`.bai` / `.crai`). CRAM needs a reference FASTA, selected in View or through **Set as reference** in the explorer.

### Align

Map FASTA/FASTQ reads to a reference genome using minimap2, with optional sorting and indexing via samtools. Output as SAM, BAM, or CRAM.

References can be:

- Downloaded from NCBI RefSeq (human, mouse, zebrafish, Arabidopsis, *E. coli*, and more)
- Loaded from a local FASTA on disk
- Set from the file explorer via **Set as reference**

## Prerequisites

- [Rust](https://rustup.rs/)
- [Node.js LTS](https://nodejs.org/)
- Visual Studio **Desktop development with C++** build tools (for Rust on Windows)

### External tools (align & CRAM)

Alignment and CRAM conversion use **minimap2** and **samtools**. Pre-built Windows binaries and their runtime DLLs are included in `src-tauri/binaries/` (committed to the repo), so users do not need MSYS2 or a compiler to run the full app.

See [`src-tauri/binaries/README.txt`](src-tauri/binaries/README.txt) for details on the bundled files. During development the app also checks the system `PATH` if local binaries are missing.

## Quick start

```powershell
git clone <repo-url>
cd HelixGT
npm install
npm run tauri dev
```

The first run compiles Rust dependencies and may take several minutes.

## Do you need to compile every time?

**No.**

| Command | When to use | What happens |
|---------|-------------|--------------|
| `npm run tauri dev` | Daily development | Rebuilds only what changed; hot-reloads the Svelte UI |
| `npm run tauri build` | Release build only | Builds installer under `target/release/bundle/` |
| `npm run release` | **Sharing / installers** | Builds, then copies installer + portable exe into a top-level **`dist/`** folder |
| `npm run collect-release` | After a build already finished | Only re-copies artifacts into `dist/` (no rebuild) |

### Where is the installer?

Tauri’s native output path is deep under the target tree. Use **`npm run release`** so you don’t have to dig:

```
HelixGT/
└── dist/                          ← open this folder after `npm run release`
    ├── README.txt
    ├── HelixGT.exe                ← portable executable
    ├── HelixGT_*_x64-setup.exe    ← NSIS installer (recommended for others)
    └── HelixGT_*_x64_en-US.msi    ← MSI installer (if produced)
```

Raw Tauri paths (if you need them):

- Portable exe: `target/release/HelixGT.exe`
- Installers: `target/release/bundle/nsis/` and `…/bundle/msi/`

During development you usually leave `tauri dev` running. Edit Svelte files and the UI refreshes automatically. Rust changes trigger a quick rebuild of the backend.

You only do a full release build when you want a distributable app.

## Project layout

```
HelixGT/
├── crates/converter-core/   # Rust engine: convert, merge, align, references
├── src/                     # Svelte frontend
├── src-tauri/               # Tauri shell, filesystem commands, reference cache
│   └── binaries/            # minimap2 + samtools (see README.txt)
├── test/                    # Local test fixtures (gitignored)
└── package.json
```

## Usage

1. Launch with `npm run tauri dev`.
2. Use the mode tabs: **Convert**, **Merge**, **Align**, or **View**.
3. Drop files onto the input panel or click **Browse**.
4. Configure output options and click **Run** (or open a sequence in **View**).

The built-in file explorer lets you browse, rename, and reveal output files. Activity and tool logs are shown in the panel on the right.

## Limitations

- FASTA → GenBank is not supported.
- Large reference downloads (e.g. human genome ~950 MB compressed) take time and need a stable connection.
- Alignment of very large FASTQ files can take hours; CRAM output is recommended for storage efficiency.

## License

MIT
## Verification

- `npm run check` — Svelte / TypeScript diagnostics.
- `npm test` — viewer coordinate, cache, fetch-window, and read-packing regressions.
- `cargo test -p converter-core` — engine regressions, including coverage against a per-base oracle and concurrent subprocess output.
- `cargo check -p helixgt` — desktop command integration.

See [the project review](PROJECT_REVIEW.md) for findings, remaining limitations, and suggested next improvements.
