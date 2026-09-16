# HelixGT project review — 2026-09-16

The existing philosophy is worth keeping: a compact Windows workbench with a visual View mode, table/QC-oriented Analyze mode, and conversion/alignment tools close at hand. Svelte, Tauri, Rust and indexed genomic I/O are a suitable foundation. A framework rewrite would not resolve the problems found here.

This review covered the mode structure, viewer fetching/rendering helpers, Rust view APIs, conversion/merge/alignment paths, subprocess runner, QC command boundary, filesystem integration, preferences and release collection. Detailed fixes and regression tests concentrate on the issues below; this is not an exhaustive certification of every format or UI path.

## Changes made

| Area | Finding and change |
| --- | --- |
| Coverage | A read wholly inside a bin could cancel its own difference-array contribution. Other overlaps could miss the last bin or depend on CIGAR segmentation. Coverage now uses mean aligned depth, with proportional contributions at bin edges and constant work per aligned block. |
| Overview | BAM tile boundaries could double-count reads; CRAM overview discarded depth values. Tiles now retain actual depth and clip boundary reads. Errors are surfaced rather than silently producing zero depth. |
| View loading | Mutable contig/filter/reference state was read after asynchronous requests. Results now require matching request identity and are merged into current tracks, preserving track removals and visibility. Annotation and reference responses also reject stale identities. |
| Responsiveness | Local alignment results no longer wait for whole-contig overview scans. Read requests use tight windows instead of the padded coverage region, reducing read-detail payloads and sampling dilution. CRAM now uses samtools indexed regional decoding; see the measured investigation below. |
| Reactive scheduling | Cache/result reads are excluded from the load effect's dependency tracking. Failed overview attempts are not retried continuously. |
| Zoom/cache planning | Coverage requests contain the full viewport even above 2 Mb. Bin counts account for padding, and base-level windows avoid excessive padding that made freshly fetched caches immediately too coarse. |
| Navigation | Cross-contig locus input uses the destination's length and resets old tracks. Fractional/unsafe coordinates and starts beyond the contig are rejected. |
| Read packing | Reads omitted by the packing input budget are now included in the hidden count. |
| Annotation memory | Range queries count all matches but clone only the bounded result set. |
| Analyze | FASTQ QC runs in a blocking worker rather than the synchronous Tauri command handler. |
| External tools | Stdout/stderr are drained concurrently, including without logging, to prevent pipe deadlocks and premature pipe closure. |
| Merge | Output filenames cannot escape the selected folder or resolve to an input file that would be truncated before reading. |
| Packaging | Release collection obtains the Cargo workspace target directory from metadata. |

The pre-existing uncommitted coverage-filter work in View was retained and extended to include reference identity.

## Remaining priorities

1. **Measure large datasets.** Add reproducible human-genome BAM/CRAM performance fixtures and record cold/warm load latency, pan frame time, memory, and cancellation latency. Automated checks here do not establish a real-world speedup factor.
2. **Explicit approximation metadata.** Whole-contig BAM overview tiles are capped; coverage windows over 2 Mb can also be capped. The manual now explains this. A future API should expose scan counts and an approximation flag directly on each track. Sampling should cover each tile spatially, rather than favoring its earliest reads.
3. **Cancellation of obsolete work.** Stale responses are rejected, but old disk scans still finish. Viewer cancellation and priority scheduling would improve rapid navigation across many large tracks. The shared external runner still waits for child completion; cancellation must reach running tools as well as record loops.
4. **Cache lifetime.** Path-based document/index caches should include file modification identity and a memory budget, particularly when a file is overwritten or many assemblies are opened.
5. **Portable format fixtures.** Some existing BAM tests return early when local fixtures are absent. Add generated BAM/CRAM fixtures with known references, mixed CIGARs, flags and aliases so CI does not depend on local data.
6. **Product additions after these foundations.** Saved sessions/loci, gene-name search, and a dedicated variant track would deepen the visual browser while keeping Analyze focused on tables. These are recommendations, not features implemented in this change.
7. **Warnings and accessibility.** Existing frontend warnings include unused CSS, dialog semantics and missing Node type definitions. Address these in a focused UI pass with keyboard testing.

## Verification and limits

Regression coverage includes a per-base coverage oracle across uneven bin widths, split CIGAR equivalence, intron exclusion, annotation truncation counts, concurrent subprocess pipes, merge input preservation, viewport/cache planning, long-read overlap, hidden counts and locus validation.

Run `npm test`, `npm run check`, `cargo test -p converter-core`, `cargo check -p helixgt`, and `npm run build`. A native interactive smoke test and a large-genome benchmark remain necessary before describing this as release-ready or faster than IGV. No installer was built and no performance comparison with IGV was performed.

Verification completed: 5 frontend regression tests and 32 Rust tests passed; desktop cargo check and production frontend build passed; Svelte check reported 0 errors and the same 45 pre-existing warnings. The desktop check retained one pre-existing unused-method warning. `git diff --check` passed. Cargo metadata confirmed the workspace target directory used by the packaging fix.


## Follow-up: slow CRAM loading reproduced and fixed

The installed noodles-cram 0.73.0 `io/reader/query.rs` walks CRAI entries matching the reference ID but never filters those entries by genomic interval. It decodes their containers and only then filters individual records. Small queries therefore perform chromosome-scale decoding. Its reference adapter also loads a complete chromosome into a fresh repository for each request. The former native path was removed from interactive coverage/read requests; BAM remains native, and CRAM uses bundled samtools with its CRAI index.

Measured locally against the user's approximately 51 GB CRAM and indexed hg38 FASTA, using the debug engine API:

| Query | Observed time |
| --- | --- |
| Old native coverage, chr1:1–125,000 | Still running after more than 10 seconds; benchmark process stopped |
| Fixed coverage, same region, 2,400 bins | 0.181 s first query; 0.170 s repeat |
| Fixed read query, chr1:10,001–11,500, including sequences | 0.082 s first query; 0.072 s repeat; 166 reads |

These are backend timings under the local filesystem cache conditions, not cold-disk or end-to-end frame timings. Reproduce with `cargo run -p converter-core --example view_timing -- ALIGNMENT REFERENCE chr1 0 125000` (append `reads` to time the read API). Run from the repository root so the example locates the bundled samtools binary.

Whole-contig depth scans are now opt-in under Display, avoiding automatic background chromosome-scale work when opening a local locus. The samtools streaming wrapper concurrently drains stderr, reaps processes and surfaces nonzero exits rather than treating failures as empty coverage. A generated two-contig CRAM regression test checks coordinates, aliases, sequences, bin depth, unknown contigs and subprocess errors; it runs when the bundled samtools executable is available.
