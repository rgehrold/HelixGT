# Universal Gene Tool

A native Windows genomics file converter built with **Rust**, **Tauri**, **Svelte**, and **noodles**.

## Supported formats

- **Sequence:** FASTA, FASTQ, GenBank
- **Annotation:** GFF, BED
- **Alignment:** SAM, BAM, CRAM (CRAM writing not yet supported)
- **Variants:** VCF

Common conversions include FASTA ↔ FASTQ, SAM ↔ BAM, CRAM → SAM/BAM, and GenBank → FASTA.

## Prerequisites

- [Rust](https://rustup.rs/)
- [Node.js LTS](https://nodejs.org/)
- Visual Studio **Desktop development with C++** build tools (for Rust on Windows)

## Quick start

```powershell
cd C:\Users\Robin\Projects\Universal_Gene_Tool
npm install
npm run tauri dev
```

The first run compiles Rust dependencies and may take several minutes.

## Do you need to compile every time?

**No.**

| Command | When to use | What happens |
|---------|-------------|--------------|
| `npm run tauri dev` | Daily development | Rebuilds only what changed; hot-reloads the Svelte UI |
| `npm run tauri build` | Release / sharing | Creates a standalone `.exe` installer in `src-tauri/target/release/bundle/` |

During development you usually leave `tauri dev` running. Edit Svelte files and the UI refreshes automatically. Rust changes trigger a quick rebuild of the backend.

You only do a full release build when you want a distributable app.

## Project layout

```
Universal_Gene_Tool/
├── crates/converter-core/   # Rust conversion engine (noodles)
├── src/                     # Svelte frontend
├── src-tauri/               # Tauri desktop shell + commands
└── package.json
```

## Usage

1. Launch the app with `npm run tauri dev`.
2. Drop files onto the input panel or click **Browse**.
3. Choose an output format and destination folder.
4. Click **Run conversion**.

## Notes

- Text outputs can be gzip-compressed with the checkbox.
- CRAM output requires a reference FASTA and is not implemented yet.
- FASTA → GenBank is not supported yet.

## License

MIT