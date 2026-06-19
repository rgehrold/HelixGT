# Universal Gene Tool

A native Windows desktop app for genomics file conversion, merging, and read alignment. Built with **Rust**, **Tauri**, **Svelte**, and **noodles**, with **minimap2** and **samtools** for alignment and CRAM support.

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
cd Universal_Gene_Tool
npm install
npm run tauri dev
```

The first run compiles Rust dependencies and may take several minutes.

## Do you need to compile every time?

**No.**

| Command | When to use | What happens |
|---------|-------------|--------------|
| `npm run tauri dev` | Daily development | Rebuilds only what changed; hot-reloads the Svelte UI |
| `npm run tauri build` | Release / sharing | Creates a standalone installer in `src-tauri/target/release/bundle/` |

During development you usually leave `tauri dev` running. Edit Svelte files and the UI refreshes automatically. Rust changes trigger a quick rebuild of the backend.

You only do a full release build when you want a distributable app.

## Project layout

```
Universal_Gene_Tool/
├── crates/converter-core/   # Rust engine: convert, merge, align, references
├── src/                     # Svelte frontend
├── src-tauri/               # Tauri shell, filesystem commands, reference cache
│   └── binaries/            # minimap2 + samtools (see README.txt)
├── test/                    # Local test fixtures (gitignored)
└── package.json
```

## Usage

1. Launch with `npm run tauri dev`.
2. Use the mode tabs: **Convert**, **Merge**, or **Align**.
3. Drop files onto the input panel or click **Browse**.
4. Configure output options and click **Run**.

The built-in file explorer lets you browse, rename, and reveal output files. Activity and tool logs are shown in the panel on the right.

## Limitations

- FASTA → GenBank is not supported.
- Large reference downloads (e.g. human genome ~950 MB compressed) take time and need a stable connection.
- Alignment of very large FASTQ files can take hours; CRAM output is recommended for storage efficiency.

## License

MIT