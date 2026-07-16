/** Structured content for the built-in HelixGT manual. */

export type ManualBlock =
  | { type: "p"; text: string }
  | { type: "h3"; text: string }
  | { type: "ul"; items: string[] }
  | { type: "ol"; items: string[] }
  | { type: "note"; text: string }
  | { type: "tip"; text: string }
  | { type: "table"; headers: string[]; rows: string[][] }
  | { type: "kbd"; items: { keys: string; desc: string }[] };

/**
 * Inline wiki markup (Wikipedia-style) inside any string field:
 *   [[page-id]]           → link labeled with that page's title
 *   [[page-id|label]]     → link with a custom label
 */
export type WikiSegment =
  | { type: "text"; text: string }
  | { type: "link"; pageId: string; label?: string };

const WIKI_LINK_RE = /\[\[([^\]|#\n]+)(?:\|([^\]\n]+))?\]\]/g;

export function parseWikiText(text: string): WikiSegment[] {
  const segments: WikiSegment[] = [];
  let last = 0;
  const re = new RegExp(WIKI_LINK_RE.source, "g");
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    if (match.index > last) {
      segments.push({ type: "text", text: text.slice(last, match.index) });
    }
    segments.push({
      type: "link",
      pageId: match[1].trim(),
      label: match[2]?.trim() || undefined,
    });
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    segments.push({ type: "text", text: text.slice(last) });
  }
  return segments.length > 0 ? segments : [{ type: "text", text }];
}

/** Strip wiki markup for plain-text search matching. */
export function plainWikiText(text: string): string {
  return text.replace(WIKI_LINK_RE, (_full, id: string, label?: string) => label?.trim() || id.trim());
}

export type ManualCategory = {
  id: string;
  title: string;
};

export type ManualPage = {
  id: string;
  categoryId: string;
  title: string;
  keywords: string[];
  blocks: ManualBlock[];
};

export const MANUAL_CATEGORIES: ManualCategory[] = [
  { id: "start", title: "Getting started" },
  { id: "browser", title: "Input browser" },
  { id: "convert", title: "Convert" },
  { id: "merge", title: "Merge" },
  { id: "align", title: "Align" },
  { id: "view", title: "View" },
  { id: "analyze", title: "File analysis" },
  { id: "formats", title: "File formats" },
  { id: "ref", title: "References & CRAM" },
  { id: "logs", title: "Logs" },
  { id: "ref-sheet", title: "Reference" },
];

export const MANUAL_PAGES: ManualPage[] = [
  // ── Getting started ──────────────────────────────────────────────
  {
    id: "welcome",
    categoryId: "start",
    title: "Welcome to HelixGT",
    keywords: ["intro", "overview", "about", "features"],
    blocks: [
      {
        type: "p",
        text: "HelixGT is a native Windows desktop app for everyday genomics file work: convert formats, merge samples, align reads with minimap2, and browse sequences, annotations, and alignments in an IGV-style View pane.",
      },
      {
        type: "h3",
        text: "What you can do",
      },
      {
        type: "ul",
        items: [
          "Convert between FASTA, FASTQ, GenBank, GFF, BED, SAM, BAM, CRAM, and VCF (including gzip text outputs).",
          "Merge multiple files of the same type into one output.",
          "Align FASTA/FASTQ reads to a reference (local FASTA or NCBI RefSeq download) with optional sort, index, and markdup.",
          "View reference bases, GFF/BED features, coverage, and individual BAM/CRAM reads with inspectors and filters.",
        ],
      },
      {
        type: "h3",
        text: "How help works",
      },
      {
        type: "p",
        text: "Throughout the app, a small faint i next to a control opens this manual on the matching topic. Use Help → User manual in the menu bar to open the full guide, or search from the left sidebar. Blue underlined links inside articles jump to related pages (for example [[format-bam|BAM]] or [[format-alignments-compared|SAM vs BAM vs CRAM]]).",
      },
      {
        type: "tip",
        text: "Alignment and CRAM conversion use bundled minimap2 and samtools when present under the app binaries. Convert/Merge of pure text and BAM usually need no extra tools.",
      },
    ],
  },
  {
    id: "quick-start",
    categoryId: "start",
    title: "Quick start",
    keywords: ["tutorial", "beginner", "first run", "workflow"],
    blocks: [
      {
        type: "ol",
        items: [
          "Browse or drop files into the Input browser on the left.",
          "Choose a mode tab: Convert, Merge, Align, or View.",
          "Set an output folder (or right-click a folder → Set as output folder). Align needs a reference; View opens files in place.",
          "Run the job (or Open selected in View). Open Logs for activity messages and tool streams.",
        ],
      },
      {
        type: "h3",
        text: "Typical workflows",
      },
      {
        type: "ul",
        items: [
          "SAM/BAM/CRAM round-trips: Convert mode with optional reference for CRAM.",
          "Combine per-lane FASTQs: Merge mode with gzip on if inputs are compressed.",
          "Map Illumina WGS: Align → reference download or local FASTA → BAM/CRAM + sort + index.",
          "Inspect a locus: View → open FASTA + GFF + indexed BAM, jump to coordinate, click coverage or reads.",
        ],
      },
      {
        type: "note",
        text: "Preferences such as mode, output folders per mode, and browser width are remembered between sessions.",
      },
    ],
  },
  {
    id: "interface",
    categoryId: "start",
    title: "Interface overview",
    keywords: ["layout", "ui", "menubar", "panels", "theme", "dark", "light"],
    blocks: [
      {
        type: "h3",
        text: "Menu bar",
      },
      {
        type: "ul",
        items: [
          "File — Print and Exit.",
          "View — Dark/light mode, blue/orange palette, restore standard layout.",
          "About — Application summary.",
          "Help — Opens this manual (and deep links to common chapters).",
        ],
      },
      {
        type: "h3",
        text: "Main workspace",
      },
      {
        type: "ul",
        items: [
          "Left: Input browser — filesystem tree, multi-select, drag-and-drop, context menu.",
          "Center divider: drag to resize; shrink the browser when you need more room for Tools/View.",
          "Right: Tools panel with a left rail of modes (Convert, Merge, Align, View, Analyze) and the active tool body.",
        ],
      },
      {
        type: "p",
        text: "Click the HelixGT wordmark to toggle the blue/orange color palette. Use View menu for dark vs light mode. Open Logs from the top menu bar (right side).",
      },
      {
        type: "tip",
        text: "Logs is a drawer over the workspace (not a processing mode). Esc closes the Logs drawer; the manual closes with Esc as well.",
      },
    ],
  },

  // ── Input browser ────────────────────────────────────────────────
  {
    id: "input-browser",
    categoryId: "browser",
    title: "Input browser",
    keywords: ["files", "explorer", "drop", "right-click", "select", "reveal"],
    blocks: [
      {
        type: "p",
        text: "The Input browser is how you pick inputs for Convert, Merge, and Align, and how you open files into View. Drop files or folders onto the panel, or expand drives and folders in the tree.",
      },
      {
        type: "h3",
        text: "Selection",
      },
      {
        type: "ul",
        items: [
          "Click a file to select it; Ctrl/Shift click for multi-select where the tree supports it.",
          "Selected paths appear as the active inputs for the current mode.",
          "After Convert/Merge/Align finishes, HelixGT can refresh the tree and reveal outputs.",
        ],
      },
      {
        type: "h3",
        text: "Context menu (right-click)",
      },
      {
        type: "ul",
        items: [
          "Set as output folder — on a directory; applies to Convert/Merge/Align output paths.",
          "Set as reference — on a FASTA; used for Align and for CRAM convert.",
          "Open in View — supported sequence, annotation, and alignment files.",
          "Other explorer actions (rename, reveal in system file manager) when available.",
        ],
      },
      {
        type: "note",
        text: "While a long job is running, the browser may disable some interactions so selection cannot change mid-run.",
      },
    ],
  },

  // ── Convert ──────────────────────────────────────────────────────
  {
    id: "convert",
    categoryId: "convert",
    title: "Convert mode",
    keywords: ["convert", "batch", "format", "sam", "bam", "fasta", "fastq", "vcf"],
    blocks: [
      {
        type: "p",
        text: "Convert transforms each selected input into the chosen output format in the output folder. Text formats can be gzip-compressed; BAM and CRAM are always binary-compressed internally.",
      },
      {
        type: "h3",
        text: "Supported directions (high level)",
      },
      {
        type: "ul",
        items: [
          "Sequence: FASTA ↔ FASTQ, GenBank → FASTA (FASTA → GenBank is not supported).",
          "Annotation: GFF, BED (and related text forms).",
          "Alignments: SAM ↔ BAM ↔ CRAM (CRAM needs a reference FASTA).",
          "Variants: VCF (including compression options where applicable).",
        ],
      },
      {
        type: "h3",
        text: "Steps",
      },
      {
        type: "ol",
        items: [
          "Select one or more input files in the browser.",
          "Choose Output format, folder, optional prefix, and gzip if needed.",
          "If converting to/from CRAM, set Reference FASTA.",
          "Optionally run Preflight check, then Convert. Cancel stops a running batch when possible.",
        ],
      },
      {
        type: "tip",
        text: "Preflight validates paths, formats, output folder, and samtools availability for CRAM, and estimates total input size.",
      },
    ],
  },
  {
    id: "convert-output-folder",
    categoryId: "convert",
    title: "Output folder (Convert)",
    keywords: ["output", "directory", "folder", "destination"],
    blocks: [
      {
        type: "p",
        text: "Destination directory for converted files. Type a path, use Choose…, or right-click a folder in the Input browser and pick Set as output folder.",
      },
      {
        type: "note",
        text: "HelixGT remembers a separate output folder per mode (Convert / Merge / Align) when you switch tabs.",
      },
    ],
  },
  {
    id: "convert-prefix",
    categoryId: "convert",
    title: "Filename prefix",
    keywords: ["prefix", "rename", "batch tag", "basename"],
    blocks: [
      {
        type: "p",
        text: "Optional string prepended to each output basename (for example run42_sample.fasta). Useful to tag a batch without renaming inputs by hand.",
      },
      {
        type: "tip",
        text: "The original stem is kept after the prefix; the extension follows the chosen output format (and .gz when compression is on).",
      },
    ],
  },
  {
    id: "convert-gzip",
    categoryId: "convert",
    title: "Gzip compress text outputs",
    keywords: ["gzip", "compress", ".gz", "text"],
    blocks: [
      {
        type: "p",
        text: "When enabled, text formats such as FASTA, FASTQ, SAM, GFF, and VCF are written with gzip compression (.gz). Binary BAM and CRAM are always compressed internally and do not use this toggle.",
      },
      {
        type: "note",
        text: "CRAM output disables the gzip control because CRAM is already a compressed container format.",
      },
    ],
  },
  {
    id: "convert-cram-reference",
    categoryId: "convert",
    title: "Reference FASTA for CRAM (Convert)",
    keywords: ["cram", "reference", "fasta", "decode", "encode"],
    blocks: [
      {
        type: "p",
        text: "CRAM stores differences against a reference FASTA. Converting to or from CRAM requires that reference so bases can be reconstructed or encoded correctly.",
      },
      {
        type: "ul",
        items: [
          "Set the path in the Convert pane when the UI shows Reference FASTA (required for CRAM).",
          "Or right-click a FASTA in the browser → Set as reference.",
          "HelixGT also looks for a matching FASTA next to each CRAM file when possible.",
        ],
      },
      {
        type: "tip",
        text: "Use the same assembly (e.g. GRCh38) that was used when the CRAM was created. A mismatched reference produces wrong sequences or conversion errors.",
      },
    ],
  },
  {
    id: "convert-preflight",
    categoryId: "convert",
    title: "Preflight check",
    keywords: ["preflight", "validate", "check", "estimate"],
    blocks: [
      {
        type: "p",
        text: "Preflight checks that inputs exist, formats are known, the output folder is set, and that samtools is available when CRAM is involved. It also estimates total input size so you can spot huge batches early.",
      },
      {
        type: "p",
        text: "Results appear in the Convert pane status area and in Logs → Activity. Fix listed issues before starting Convert.",
      },
    ],
  },
  {
    id: "convert-fastq-qc",
    categoryId: "convert",
    title: "FASTQ QC",
    keywords: ["qc", "quality", "fastq", "stats", "n content"],
    blocks: [
      {
        type: "p",
        text: "When exactly one FASTQ is selected, FASTQ QC scans it for read count, base count, length statistics, and N content. Results are written to Logs → Activity.",
      },
      {
        type: "note",
        text: "This is a lightweight summary, not a full FastQC-style multiplot report. Large files may take a while to stream.",
      },
    ],
  },

  // ── Merge ────────────────────────────────────────────────────────
  {
    id: "merge",
    categoryId: "merge",
    title: "Merge mode",
    keywords: ["merge", "concatenate", "combine", "lanes"],
    blocks: [
      {
        type: "p",
        text: "Merge concatenates multiple selected files of the same detected type into a single output. Supported types include FASTA, FASTQ, SAM, BAM, CRAM, GFF, BED, VCF, and GenBank.",
      },
      {
        type: "ol",
        items: [
          "Select two or more compatible inputs.",
          "Confirm the suggested merged filename (or edit it).",
          "Set output folder and gzip if appropriate.",
          "Run Merge. Outputs can be revealed in the browser when finished.",
        ],
      },
      {
        type: "h3",
        text: "CRAM note",
      },
      {
        type: "p",
        text: "CRAM files are concatenated using the first file’s header. For full reference-aware re-encoding, convert to BAM first, then merge or re-encode to CRAM.",
      },
    ],
  },
  {
    id: "merge-output-folder",
    categoryId: "merge",
    title: "Output folder (Merge)",
    keywords: ["output", "folder", "merge destination"],
    blocks: [
      {
        type: "p",
        text: "Where the merged file is written. Type a path, use Choose…, or right-click a folder in the Input browser → Set as output folder.",
      },
    ],
  },
  {
    id: "merge-filename",
    categoryId: "merge",
    title: "Merged filename",
    keywords: ["filename", "name", "prefix", "suffix", "suggestion"],
    blocks: [
      {
        type: "p",
        text: "HelixGT suggests a filename from shared prefixes and suffixes in the input names. Edit freely, but keep the same extension as the inputs (including .gz if you are compressing).",
      },
      {
        type: "tip",
        text: "If inputs differ only by lane IDs (e.g. sample_L001.fastq.gz, sample_L002.fastq.gz), the suggestion often collapses to a clean sample-level name.",
      },
    ],
  },
  {
    id: "merge-gzip",
    categoryId: "merge",
    title: "Gzip compress merge output",
    keywords: ["gzip", "merge", "compress"],
    blocks: [
      {
        type: "p",
        text: "When on, text merges are gzipped. The toggle defaults to on if every input already ends with .gz. Binary BAM/CRAM merges ignore this control.",
      },
    ],
  },

  // ── Align ────────────────────────────────────────────────────────
  {
    id: "align",
    categoryId: "align",
    title: "Align mode",
    keywords: ["align", "minimap2", "mapping", "wgs", "pipeline"],
    blocks: [
      {
        type: "p",
        text: "Align maps selected FASTA/FASTQ reads to a reference genome using minimap2, then pipes through samtools for headers, sorting, BAM/CRAM writing, indexing, and optional markdup.",
      },
      {
        type: "h3",
        text: "Requirements",
      },
      {
        type: "ul",
        items: [
          "minimap2 and samtools available (bundled with the app when binaries are present).",
          "A loaded reference (NCBI download into the session cache, or local FASTA).",
          "One or more read files selected in the Input browser.",
          "Output folder and format (SAM, BAM, or CRAM).",
        ],
      },
      {
        type: "h3",
        text: "Large runs",
      },
      {
        type: "p",
        text: "Whole-genome Illumina runs can take hours and produce large intermediates. Prefer CRAM for storage efficiency, enable sort + index for IGV/View, and watch Logs → Tools for minimap2/samtools streams.",
      },
      {
        type: "note",
        text: "You can cancel a running alignment; partial outputs may remain on disk until cleaned up.",
      },
    ],
  },
  {
    id: "align-reference",
    categoryId: "align",
    title: "Reference genome",
    keywords: ["reference", "ncbi", "refseq", "genome", "mmi"],
    blocks: [
      {
        type: "p",
        text: "The reference genome is the template against which the sequencing reads are aligned.",
      },
      {
        type: "tip",
        text: "For large whole-genome alignments prefer CRAM output. Building a .mmi index first helps if you will align many samples to the same reference.",
      },
    ],
  },
  {
    id: "align-sam-gzip",
    categoryId: "align",
    title: "Gzip compress SAM output",
    keywords: ["sam", "gzip", "align output"],
    blocks: [
      {
        type: "p",
        text: "Only applies when output format is SAM. Writes a .sam.gz file instead of plain text SAM. BAM and CRAM use their own compression.",
      },
    ],
  },
  {
    id: "align-preset",
    categoryId: "align",
    title: "Read preset (minimap2 -x)",
    keywords: ["preset", "-x", "sr", "ont", "hifi", "splice", "illumina"],
    blocks: [
      {
        type: "p",
        text: "Maps to minimap2’s -x presets. Pick the closest match to your library type for better sensitivity and speed.",
      },
      {
        type: "table",
        headers: ["Preset", "Typical use"],
        rows: [
          ["General", "Default long-read style mapping; good first try for mixed or standard WGS-style pipelines in this app"],
          ["Short reads (-x sr)", "Short inserts / small genomes; not ideal for huge typical Illumina WGS datasets"],
          ["Nanopore (-x map-ont)", "Oxford Nanopore long reads"],
          ["PacBio HiFi (-x map-hifi)", "HiFi / CCS accurate long reads"],
          ["RNA-seq / splice (-x splice)", "Spliced RNA alignments"],
          ["Assembly (-x asm5)", "Assembly-to-assembly alignment"],
        ],
      },
    ],
  },
  {
    id: "align-index-ref",
    categoryId: "align",
    title: "Build reference index (.mmi)",
    keywords: ["mmi", "index", "minimap2 -d"],
    blocks: [
      {
        type: "p",
        text: "Runs minimap2 -d to build a .mmi index of the reference before aligning. Helps when you repeatedly map to the same genome; optional for one-off runs.",
      },
    ],
  },
  {
    id: "align-sort",
    categoryId: "align",
    title: "Sort by coordinate",
    keywords: ["sort", "coordinate", "samtools sort", "igv"],
    blocks: [
      {
        type: "p",
        text: "Sorts alignments by chromosome and position via samtools sort. Required for IGV, HelixGT View, and most downstream tools. Also required before markdup.",
      },
    ],
  },
  {
    id: "align-secondary",
    categoryId: "align",
    title: "Report secondary alignments",
    keywords: ["secondary", "-N 0", "multimapper"],
    blocks: [
      {
        type: "p",
        text: "When off, minimap2 uses -N 0 and reports only primary alignments per read. When on, secondary hits for multi-mappers may be included (useful for some analyses; noisier in View).",
      },
    ],
  },
  {
    id: "align-index-out",
    categoryId: "align",
    title: "Create BAM/CRAM index",
    keywords: ["bai", "crai", "index", "igv"],
    blocks: [
      {
        type: "p",
        text: "Runs samtools index after BAM or CRAM output so IGV and HelixGT View can load the file without building an index themselves. Not applicable to plain SAM.",
      },
    ],
  },
  {
    id: "align-drop-unmapped",
    categoryId: "align",
    title: "Remove unmapped reads",
    keywords: ["unmapped", "-F 4", "filter"],
    blocks: [
      {
        type: "p",
        text: "Drops reads with the unmapped flag (samtools view -F 4) before writing the final file. Shrinks BAM/CRAM/SAM when you only care about mapped alignments.",
      },
    ],
  },
  {
    id: "align-markdup",
    categoryId: "align",
    title: "Mark duplicates",
    keywords: ["markdup", "duplicates", "pcr", "optical"],
    blocks: [
      {
        type: "p",
        text: "Runs samtools markdup on sorted BAM/CRAM (not SAM). Marks PCR/optical duplicates; most tools treat them as filterable by flag (0x400). Requires coordinate-sorted output—enable Sort.",
      },
      {
        type: "note",
        text: "Markdup does not remove duplicates by default; it sets a flag. Use View → Hide duplicates or downstream filters to exclude them from analysis.",
      },
    ],
  },

  // ── View ─────────────────────────────────────────────────────────
  {
    id: "view",
    categoryId: "view",
    title: "View mode",
    keywords: ["view", "igv", "browser", "tracks", "pileup"],
    blocks: [
      {
        type: "p",
        text: "View loads sequence, annotation, and alignment files on a linear canvas. Open files with Open selected, drop/select then open, or right-click → Open in View. For tabular read dumps and FASTQ QC, use [[analyze|File analysis]].",
      },
      {
        type: "h3",
        text: "Tracks",
      },
      {
        type: "ul",
        items: [
          "Reference sequence (FASTA/FASTQ) with base coloring when zoomed in.",
          "Feature track for GFF/BED intervals.",
          "Coverage histogram from samtools depth (indexed BAM/CRAM).",
          "Pileup bars for individual alignments (CIGAR, soft-clips, indels, mismatches vs reference); click a bar for the read inspector.",
        ],
      },
      {
        type: "h3",
        text: "BAM/CRAM requirements",
      },
      {
        type: "ul",
        items: [
          "Coordinate-sorted alignments.",
          "Index present: .bai for BAM, .crai for CRAM.",
          "CRAM also needs a reference FASTA (Convert reference field or Set as reference).",
        ],
      },
      {
        type: "tip",
        text: "Combine a reference FASTA, gene annotations, and an indexed BAM for the full multi-track experience at a locus of interest.",
      },
    ],
  },
  {
    id: "view-navigation",
    categoryId: "view",
    title: "Navigation & zoom",
    keywords: ["zoom", "pan", "fit", "selection", "copy", "scroll"],
    blocks: [
      {
        type: "p",
        text: "Use toolbar buttons and mouse gestures to move along the contig and change resolution.",
      },
      {
        type: "ul",
        items: [
          "Zoom in / Zoom out — change window size around the center.",
          "Fit contig — show the entire chromosome/contig.",
          "Fit selection — zoom to a dragged base range.",
          "Copy selection — copy the selected reference bases (when sequence is open).",
          "Zoom to feature / Zoom to read — jump to the selected annotation or alignment.",
        ],
      },
      {
        type: "p",
        text: "See Keyboard & mouse shortcuts for gesture details (scroll, Ctrl+scroll, shift-drag, etc.).",
      },
    ],
  },
  {
    id: "view-contig",
    categoryId: "view",
    title: "Contig selector",
    keywords: ["contig", "chromosome", "scaffold", "@SQ"],
    blocks: [
      {
        type: "p",
        text: "Chromosome or sequence name taken from the open FASTA, BAM header (@SQ), or annotation contigs. Switching contig resets the window to the start of that sequence.",
      },
      {
        type: "note",
        text: "Names must match across layers (e.g. chr1 vs 1). If features or reads do not appear, confirm contig naming matches the reference.",
      },
    ],
  },
  {
    id: "view-goto",
    categoryId: "view",
    title: "Go to coordinate",
    keywords: ["goto", "locus", "position", "1-based", "coordinate"],
    blocks: [
      {
        type: "p",
        text: "Jump to a 1-based coordinate on the current contig. HelixGT centers the viewport on that position when possible. Same convention as IGV-style locus bars.",
      },
      {
        type: "tip",
        text: "Press Enter in the Go to field or click Go after typing the position.",
      },
    ],
  },
  {
    id: "view-coverage",
    categoryId: "view",
    title: "Coverage track",
    keywords: ["coverage", "depth", "histogram", "pileup", "BAM"],
    blocks: [
      {
        type: "p",
        text: "Histogram of read depth from samtools depth over the visible window (requires an open indexed BAM/CRAM). Click a bar for bin depth details in the inspector.",
      },
      {
        type: "note",
        text: "Depth reflects reads that pass current filters (secondary, supplementary, duplicates, min MAPQ) when those filters are applied to the loaded window.",
      },
    ],
  },
  {
    id: "view-features",
    categoryId: "view",
    title: "Features track",
    keywords: ["gff", "bed", "annotation", "gene", "feature"],
    blocks: [
      {
        type: "p",
        text: "Shows GFF/BED intervals overlapping the current window. Open annotation files with Open selected or right-click → Open in View. Click a feature for details; double-click to zoom.",
      },
    ],
  },
  {
    id: "view-reads-track",
    categoryId: "view",
    title: "Pileup bars",
    keywords: ["pileup", "reads", "cigar", "soft-clip", "indel", "mismatch"],
    blocks: [
      {
        type: "p",
        text: "Canvas track of individual alignments drawn when zoomed in enough. Bars encode CIGAR structure (matches, insertions, deletions, soft-clips). Mismatches vs the open reference highlight when base resolution allows.",
      },
      {
        type: "p",
        text: "Click a bar for full read details (name, flags, MAPQ, CIGAR, sequence, qualities). Color mode is controlled by Color reads.",
      },
    ],
  },
  {
    id: "view-read-list",
    categoryId: "view",
    title: "Read list (moved)",
    keywords: ["read list", "table", "inspector", "rows", "analyze"],
    blocks: [
      {
        type: "p",
        text: "The full “Reads in view” table was removed from [[view|View]] to keep the browser focused on tracks. Use [[analyze|File analysis]] instead: open a BAM/CRAM, choose a contig and region, load reads, filter, inspect, and export TSV.",
      },
      {
        type: "tip",
        text: "In View you can still click a pileup bar for a single-read inspector. For bulk tables and MAPQ summaries, switch to Analyze.",
      },
    ],
  },
  {
    id: "view-revcomp",
    categoryId: "view",
    title: "Reverse-complement reference display",
    keywords: ["reverse", "complement", "strand", "display"],
    blocks: [
      {
        type: "p",
        text: "Displays the reverse-complement of the reference sequence window only. Does not rewrite files, change BAM coordinates, or alter annotations on disk.",
      },
    ],
  },
  {
    id: "view-filter-secondary",
    categoryId: "view",
    title: "Hide secondary alignments",
    keywords: ["secondary", "0x100", "multimapper", "filter"],
    blocks: [
      {
        type: "p",
        text: "Hides secondary alignments (SAM flag 0x100)—typical multi-mappers beyond the primary hit. Helps declutter the pileup track.",
      },
    ],
  },
  {
    id: "view-filter-supplementary",
    categoryId: "view",
    title: "Hide supplementary alignments",
    keywords: ["supplementary", "0x800", "chimeric", "split"],
    blocks: [
      {
        type: "p",
        text: "Hides supplementary alignments (flag 0x800), for example chimeric or split-read segments from structural variants or long-read multi-segment mappings.",
      },
    ],
  },
  {
    id: "view-filter-duplicates",
    categoryId: "view",
    title: "Hide duplicate reads",
    keywords: ["duplicate", "0x400", "markdup", "pcr"],
    blocks: [
      {
        type: "p",
        text: "Hides reads marked as PCR/optical duplicates (flag 0x400), usually set by samtools markdup or similar tools. Does not delete data from the BAM/CRAM file.",
      },
    ],
  },
  {
    id: "view-min-mapq",
    categoryId: "view",
    title: "Minimum MAPQ",
    keywords: ["mapq", "mapping quality", "filter", "confidence"],
    blocks: [
      {
        type: "p",
        text: "Only show reads with mapping quality ≥ this value. 0 keeps everything; 20–30 is a common filter for confident unique mappings depending on the aligner and use case.",
      },
    ],
  },
  {
    id: "view-color-by",
    categoryId: "view",
    title: "Color reads by",
    keywords: ["color", "strand", "mapq", "pair", "proper pair"],
    blocks: [
      {
        type: "ul",
        items: [
          "Strand — forward vs reverse strand alignments.",
          "MAPQ — green / amber / red bands by mapping quality.",
          "Pair status — proper pairs vs other paired or unpaired reads.",
        ],
      },
    ],
  },

  // ── File analysis ────────────────────────────────────────────────
  {
    id: "analyze",
    categoryId: "analyze",
    title: "File analysis",
    keywords: ["analyze", "analysis", "qc", "stats", "fastq", "reads", "region", "tsv", "inspect"],
    blocks: [
      {
        type: "p",
        text: "[[analyze|File analysis]] inspects selected files without converting them. It replaces the old View “Reads in view” table with a dedicated workflow for QC, inventories, and region read tables.",
      },
      {
        type: "h3",
        text: "How to use",
      },
      {
        type: "ol",
        items: [
          "Select a file in the Input browser (first selected path is used if several are highlighted).",
          "Open the Analyze tool from the left rail.",
          "For CRAM, set a [[format-fasta|FASTA]] reference (Choose… or Set as reference).",
          "Click Analyze selected.",
        ],
      },
      {
        type: "h3",
        text: "What each kind does",
      },
      {
        type: "ul",
        items: [
          "[[format-fastq|FASTQ]] — read count, base count, min/mean/max length, N-content fraction.",
          "[[format-fasta|FASTA]] — contig table and total bases.",
          "[[format-gff|GFF]] / [[format-bed|BED]] — feature count and contig spans.",
          "[[format-bam|BAM]] / [[format-cram|CRAM]] — header/index summary, contig list, load reads in a region with filters, MAPQ histogram, flag tallies, full inspector, copy/export TSV.",
          "[[format-vcf|VCF]] / [[format-genbank|GenBank]] — identity hints; deeper tools may be added later.",
        ],
      },
      {
        type: "h3",
        text: "Region reads (alignments)",
      },
      {
        type: "p",
        text: "After analyzing an indexed BAM/CRAM, pick contig, 1-based start/end, and filters (secondary, supplementary, duplicates, min MAPQ). Load reads, filter the table, click a row for sequence/qualities/flags, then Copy TSV or Export TSV.",
      },
      {
        type: "tip",
        text: "For visual locus browsing keep using [[view|View]] (coverage + pileup). Use Analyze when you need bulk tables and QC numbers.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: ["[[view|View mode]]", "[[format-alignments-compared|SAM vs BAM vs CRAM]]", "[[formats|Supported formats]]"],
      },
    ],
  },

  // ── File formats ─────────────────────────────────────────────────
  {
    id: "formats",
    categoryId: "formats",
    title: "Supported formats",
    keywords: ["formats", "extensions", "overview", "gzip", "list"],
    blocks: [
      {
        type: "p",
        text: "HelixGT works with common genomics file types for convert, merge, align, and view. Use the pages in this section like encyclopedia entries: what the format is, how it is structured, how it differs from related formats, and what HelixGT can do with it. Blue links jump to related articles.",
      },
      {
        type: "table",
        headers: ["Category", "Formats", "Role"],
        rows: [
          [
            "Sequence",
            "[[format-fasta|FASTA]], [[format-fastq|FASTQ]], [[format-genbank|GenBank]]",
            "Bases (and qualities for FASTQ); GenBank also carries rich annotations",
          ],
          [
            "Annotation",
            "[[format-gff|GFF]], [[format-bed|BED]]",
            "Genes, intervals, and tracks drawn over a reference",
          ],
          [
            "Alignment",
            "[[format-sam|SAM]], [[format-bam|BAM]], [[format-cram|CRAM]]",
            "Where reads map on a reference — see [[format-alignments-compared|SAM vs BAM vs CRAM]]",
          ],
          ["Variants", "[[format-vcf|VCF]]", "SNPs, indels, and other variant calls"],
        ],
      },
      {
        type: "h3",
        text: "Compression",
      },
      {
        type: "p",
        text: "Many text formats accept gzip on input (.gz) and can be written gzipped from Convert or Merge when compress is on. [[format-bam|BAM]] and [[format-cram|CRAM]] are always compressed binary containers; they ignore the text gzip toggle.",
      },
      {
        type: "h3",
        text: "All format pages",
      },
      {
        type: "ul",
        items: [
          "[[format-fasta|FASTA]] — reference and contig sequences",
          "[[format-fastq|FASTQ]] — reads with quality scores",
          "[[format-genbank|GenBank]] — annotated sequence records",
          "[[format-gff|GFF]] — feature annotations",
          "[[format-bed|BED]] — interval tracks",
          "[[format-sam|SAM]] — text alignments",
          "[[format-bam|BAM]] — binary alignments",
          "[[format-cram|CRAM]] — reference-based alignments",
          "[[format-alignments-compared|SAM vs BAM vs CRAM]] — side-by-side comparison",
          "[[format-vcf|VCF]] — variant calls",
        ],
      },
    ],
  },
  {
    id: "format-alignments-compared",
    categoryId: "formats",
    title: "SAM vs BAM vs CRAM",
    keywords: [
      "sam",
      "bam",
      "cram",
      "comparison",
      "difference",
      "vs",
      "alignment",
      "which format",
      "binary",
      "text",
    ],
    blocks: [
      {
        type: "p",
        text: "[[format-sam|SAM]], [[format-bam|BAM]], and [[format-cram|CRAM]] all store the same kind of information: how sequencing reads (or contigs) map to a reference genome. They differ in encoding, size, speed, and tooling requirements—not in the conceptual data model.",
      },
      {
        type: "h3",
        text: "One data model, three containers",
      },
      {
        type: "p",
        text: "An alignment record typically includes a read name, bitwise FLAG, reference name and position, mapping quality (MAPQ), CIGAR string (how the read is gapped/clipped against the reference), mate information for paired reads, the read sequence, base qualities, and optional tags. Headers describe the sort order, reference sequences (@SQ), read groups (@RG), and programs (@PG).",
      },
      {
        type: "p",
        text: "[[format-sam|SAM]] writes that information as plain text. [[format-bam|BAM]] is a binary, BGZF-compressed encoding of the same records. [[format-cram|CRAM]] is also binary and compressed, but stores sequence relative to a reference [[format-fasta|FASTA]], which usually shrinks files further.",
      },
      {
        type: "h3",
        text: "Quick comparison",
      },
      {
        type: "table",
        headers: ["", "SAM", "BAM", "CRAM"],
        rows: [
          ["Human-readable?", "Yes (open in a text editor)", "No (binary)", "No (binary)"],
          ["Typical size", "Largest", "Much smaller than SAM", "Often smallest of the three"],
          ["Compression", "Optional gzip (.sam.gz)", "Built-in (BGZF blocks)", "Built-in; reference-aware"],
          ["Needs reference FASTA?", "No", "No (sequence is self-contained)", "Yes, to encode/decode"],
          ["Random access in viewers", "Poor / whole-file", "Yes, if sorted + .bai", "Yes, if sorted + .crai"],
          ["Best for", "Debugging, small tests, piping text", "General pipelines, IGV, HelixGT View", "Long-term storage of large WGS"],
        ],
      },
      {
        type: "h3",
        text: "SAM — text alignments",
      },
      {
        type: "p",
        text: "Choose [[format-sam|SAM]] when you need to inspect or edit alignments by eye, teach the format, or run a tiny experiment. Unsorted SAM is fine for streaming convert. It is a poor default for multi-gigabyte whole-genome runs: files explode in size and most genome browsers expect indexed BAM/CRAM.",
      },
      {
        type: "h3",
        text: "BAM — binary workhorse",
      },
      {
        type: "p",
        text: "[[format-bam|BAM]] is the usual interchange format. Same logical records as SAM, packed into compressed blocks. Coordinate-sort and build a .bai index for [[view|View]] or IGV. Sequence and qualities live inside the file, so you do not need the original reference just to list reads (though a matching [[format-fasta|FASTA]] is still useful for mismatch coloring and many analyses).",
      },
      {
        type: "h3",
        text: "CRAM — smaller, reference-dependent",
      },
      {
        type: "p",
        text: "[[format-cram|CRAM]] optimizes storage by not fully restating bases that match the reference. That makes the correct assembly FASTA mandatory for convert and view. Prefer CRAM for large Align outputs when disk matters; keep the reference with the project so files remain decodable years later.",
      },
      {
        type: "h3",
        text: "What HelixGT recommends",
      },
      {
        type: "ul",
        items: [
          "Everyday browse/analysis: sorted + indexed [[format-bam|BAM]] (or [[format-cram|CRAM]] + reference).",
          "Large Align jobs: [[format-cram|CRAM]] with sort + index when you have a stable reference.",
          "Debugging a few reads: [[format-sam|SAM]] or Convert BAM → SAM for a region.",
          "Round-trips: Convert freely among SAM/BAM/CRAM; CRAM always needs a reference.",
        ],
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: [
          "[[format-sam|SAM]]",
          "[[format-bam|BAM]]",
          "[[format-cram|CRAM]]",
          "[[cram-and-reference|CRAM and reference genomes]]",
          "[[formats|Supported formats]]",
        ],
      },
    ],
  },
  {
    id: "format-fasta",
    categoryId: "formats",
    title: "FASTA",
    keywords: ["fasta", "fa", "fna", "sequence", "reference", "contig", ".fasta", ".fa"],
    blocks: [
      {
        type: "p",
        text: "[[format-fasta|FASTA]] is a plain-text format for biological sequences (DNA, RNA, or protein). It is the standard way to ship a reference genome, a set of contigs, or a collection of genes. Unlike [[format-fastq|FASTQ]], it does not store per-base quality scores.",
      },
      {
        type: "h3",
        text: "What it looks like",
      },
      {
        type: "p",
        text: "Each record begins with a header line starting with “>”, then one or more lines of sequence characters. The header may contain an ID and a free-text description. Multiple records can appear in one file (multi-FASTA).",
      },
      {
        type: "ul",
        items: [
          "Example header: >chr1 Homo sapiens chromosome 1",
          "Sequence lines are usually wrapped (e.g. 60–80 characters) but may be one long line",
          "Alphabet is typically ACGTN for DNA (case may vary); proteins use amino-acid letters",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".fasta, .fa, .fna, .ffn, .faa — and .gz when gzip-compressed"],
      },
      {
        type: "h3",
        text: "How it differs from related formats",
      },
      {
        type: "ul",
        items: [
          "[[format-fastq|FASTQ]] — same idea of bases, but every read also has qualities; used for raw sequencing output, not full genomes.",
          "[[format-genbank|GenBank]] — sequence plus rich feature tables and metadata; HelixGT can extract FASTA from GenBank, not the reverse.",
          "[[format-sam|SAM]] / [[format-bam|BAM]] / [[format-cram|CRAM]] — describe where reads map on a reference; they are not substitutes for the reference FASTA itself. CRAM still needs a FASTA to decode.",
        ],
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — to/from FASTQ; copy/recompress; GenBank → FASTA. FASTA → GenBank is not supported.",
          "Merge — concatenate multiple FASTA files into one multi-record file.",
          "Align — reference genome (NCBI download, local file, or Set as reference); reads may also be FASTA.",
          "View — reference base track; mismatch coloring against open alignments when zoomed in.",
          "CRAM workflows — supply the matching assembly FASTA (see [[cram-and-reference|CRAM and reference genomes]]).",
        ],
      },
      {
        type: "tip",
        text: "Contig names after “>” should match BAM/CRAM @SQ names and GFF/BED sequence IDs (chr1 vs 1 is a common mismatch).",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: [
          "[[format-fastq|FASTQ]]",
          "[[format-genbank|GenBank]]",
          "[[format-cram|CRAM]]",
          "[[formats|Supported formats]]",
        ],
      },
    ],
  },
  {
    id: "format-fastq",
    categoryId: "formats",
    title: "FASTQ",
    keywords: ["fastq", "fq", "reads", "quality", "illumina", ".fastq", ".fq", "phred"],
    blocks: [
      {
        type: "p",
        text: "[[format-fastq|FASTQ]] is the common plain-text format for raw or demultiplexed sequencing reads. Each record carries the read sequence and a parallel string of quality scores so tools can down-weight uncertain bases during alignment and variant calling.",
      },
      {
        type: "h3",
        text: "Record structure",
      },
      {
        type: "p",
        text: "A single read is four lines:",
      },
      {
        type: "ol",
        items: [
          "Identifier line starting with “@” (instrument/run metadata often embedded)",
          "Raw bases (A/C/G/T/N…)",
          "Separator line starting with “+” (optional repeat of the id)",
          "Quality string, same length as the sequence — usually Phred+33 ASCII",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".fastq, .fq — production data is often .fastq.gz"],
      },
      {
        type: "h3",
        text: "How it differs from related formats",
      },
      {
        type: "ul",
        items: [
          "[[format-fasta|FASTA]] — sequence only, no qualities. Converting FASTQ → FASTA discards quality data.",
          "[[format-sam|SAM]] / [[format-bam|BAM]] / [[format-cram|CRAM]] — after alignment, reads live as mapped records with CIGAR, flags, and reference coordinates. FASTQ is the pre-alignment input.",
        ],
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — to FASTA or copy/recompress FASTQ; optional gzip on text output.",
          "Merge — stitch lane or split FASTQs; gzip often defaults on if every input is .gz.",
          "Align — primary input for minimap2 (FASTA reads also accepted).",
          "View — can open for base browsing; not a replacement for BAM pileup.",
          "FASTQ QC (Convert) — quick read/base/length/N stats for one selected file (Logs → Activity).",
        ],
      },
      {
        type: "note",
        text: "Large WGS FASTQs take a long time to align. Plan disk for intermediates and prefer [[format-cram|CRAM]] (or compact BAM) for final storage — see [[format-alignments-compared|SAM vs BAM vs CRAM]].",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: ["[[format-fasta|FASTA]]", "[[align|Align mode]]", "[[formats|Supported formats]]"],
      },
    ],
  },
  {
    id: "format-genbank",
    categoryId: "formats",
    title: "GenBank",
    keywords: ["genbank", "gb", "gbk", "gbff", "annotation", "ncbi"],
    blocks: [
      {
        type: "p",
        text: "[[format-genbank|GenBank]] is NCBI’s classic rich text format for a sequence together with literature, taxonomy, and a feature table (genes, CDS, source, variations, etc.). It is much heavier than a bare [[format-fasta|FASTA]] file.",
      },
      {
        type: "h3",
        text: "What is inside",
      },
      {
        type: "ul",
        items: [
          "Locus / definition / accession-style metadata at the top",
          "FEATURE table with locations and qualifiers (/gene=, /product=, …)",
          "ORIGIN section with numbered sequence",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".gb, .gbk, .gbff (sometimes .gz)"],
      },
      {
        type: "h3",
        text: "How it differs from related formats",
      },
      {
        type: "ul",
        items: [
          "[[format-fasta|FASTA]] — sequence only; use Convert GenBank → FASTA to extract bases for Align/View/CRAM.",
          "[[format-gff|GFF]] / [[format-bed|BED]] — annotation-first tracks over a separate FASTA; better for HelixGT View feature tracks than raw GenBank.",
        ],
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — GenBank → FASTA.",
          "Merge — same-type GenBank merges.",
          "Limitation — FASTA → GenBank is not supported (no synthetic GenBank writer).",
        ],
      },
      {
        type: "tip",
        text: "For IGV-style multi-track viewing, prefer FASTA + GFF/BED rather than browsing GenBank as a track.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: ["[[format-fasta|FASTA]]", "[[format-gff|GFF]]", "[[formats|Supported formats]]"],
      },
    ],
  },
  {
    id: "format-gff",
    categoryId: "formats",
    title: "GFF",
    keywords: ["gff", "gff3", "gtf", "annotation", "gene", "feature"],
    blocks: [
      {
        type: "p",
        text: "[[format-gff|GFF]] (General Feature Format), most often GFF3, describes genomic features as lines of tab-separated fields. It answers “what genes/exons/regions exist on this sequence?” rather than storing the sequence itself—that usually lives in a companion [[format-fasta|FASTA]].",
      },
      {
        type: "h3",
        text: "Columns (GFF3)",
      },
      {
        type: "ol",
        items: [
          "seqid — chromosome/contig name (must match FASTA / BAM headers)",
          "source — tool or database that produced the feature",
          "type — gene, exon, CDS, region, …",
          "start / end — typically 1-based inclusive coordinates",
          "score — optional numeric score or “.”",
          "strand — + / − / .",
          "phase — for CDS (0/1/2) or “.”",
          "attributes — semicolon-separated tags such as ID=…;Name=…;Parent=…",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".gff, .gff3 (and .gz)"],
      },
      {
        type: "h3",
        text: "How it differs from related formats",
      },
      {
        type: "ul",
        items: [
          "[[format-bed|BED]] — simpler intervals (often 0-based half-open); fewer typed attributes. Both can draw tracks in View.",
          "[[format-genbank|GenBank]] — bundles sequence + features in one file; GFF keeps annotations separate from FASTA.",
          "[[format-vcf|VCF]] — variants (alleles), not gene models.",
        ],
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert / Merge — text annotation; optional gzip.",
          "View — feature track; click for details, double-click to zoom.",
        ],
      },
      {
        type: "note",
        text: "If features never appear, check that column-1 seqids match the open reference contigs (same chr1 vs 1 issue as alignments).",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: ["[[format-bed|BED]]", "[[view|View mode]]", "[[formats|Supported formats]]"],
      },
    ],
  },
  {
    id: "format-bed",
    categoryId: "formats",
    title: "BED",
    keywords: ["bed", "interval", "track", "ucsc", "peaks", "0-based"],
    blocks: [
      {
        type: "p",
        text: "[[format-bed|BED]] (Browser Extensible Data) is a lightweight interval format popularized by the UCSC Genome Browser. At minimum it stores chrom, start, and end for each region—ideal for peaks, capture targets, blacklist regions, or custom tracks.",
      },
      {
        type: "h3",
        text: "Coordinates",
      },
      {
        type: "p",
        text: "Classic BED uses 0-based, half-open intervals: start is the first base included (counting from 0), end is one past the last included base. That differs from GFF’s usual 1-based inclusive system—so the same biological region can look “off by one” if you mix conventions without converting.",
      },
      {
        type: "h3",
        text: "Common columns",
      },
      {
        type: "ul",
        items: [
          "Required: chrom, chromStart, chromEnd",
          "Optional: name, score, strand, thickStart, thickEnd, itemRgb, block fields (BED12), …",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".bed (and .gz)"],
      },
      {
        type: "h3",
        text: "How it differs from related formats",
      },
      {
        type: "ul",
        items: [
          "[[format-gff|GFF]] — richer typed features and attributes; usually 1-based inclusive.",
          "[[format-vcf|VCF]] — variant alleles at positions, not arbitrary interval tracks.",
          "[[format-fasta|FASTA]] — sequence only; BED never contains the bases themselves.",
        ],
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert / Merge — text intervals; optional gzip.",
          "View — feature track similar to GFF (inspect / zoom).",
        ],
      },
      {
        type: "tip",
        text: "If a BED track looks shifted versus GFF in View, compare 0-based half-open vs 1-based inclusive conventions from your source tool.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: ["[[format-gff|GFF]]", "[[view|View mode]]", "[[formats|Supported formats]]"],
      },
    ],
  },
  {
    id: "format-sam",
    categoryId: "formats",
    title: "SAM",
    keywords: ["sam", "alignment", "text", "header", "@SQ", "cigar", "flag"],
    blocks: [
      {
        type: "p",
        text: "[[format-sam|SAM]] (Sequence Alignment/Map) is the text specification for read alignments. It is the human-readable sibling of [[format-bam|BAM]] and the conceptual parent of both BAM and [[format-cram|CRAM]]: those formats encode the same record types more compactly.",
      },
      {
        type: "h3",
        text: "What a SAM file contains",
      },
      {
        type: "ul",
        items: [
          "Header lines starting with @ — e.g. @HD (version, sort order), @SQ (reference sequences and lengths), @RG (read groups), @PG (programs)",
          "Alignment lines — tab-separated fields: QNAME, FLAG, RNAME, POS, MAPQ, CIGAR, RNEXT, PNEXT, TLEN, SEQ, QUAL, then optional TAG:TYPE:VALUE fields",
        ],
      },
      {
        type: "h3",
        text: "Important fields (plain language)",
      },
      {
        type: "ul",
        items: [
          "FLAG — bitfield (paired, unmapped, reverse strand, secondary, duplicate, supplementary, …)",
          "RNAME / POS — which reference contig and 1-based leftmost mapping position",
          "MAPQ — mapping confidence (higher is better; 0 often means unavailable/unique mapping not claimed)",
          "CIGAR — operations like match, insertion, deletion, soft-clip describing how SEQ lines up to the reference",
          "SEQ / QUAL — the read bases and qualities (may be “*” in some specialized records)",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".sam — optional .sam.gz if gzip-compressed as text"],
      },
      {
        type: "h3",
        text: "SAM vs BAM vs CRAM",
      },
      {
        type: "p",
        text: "SAM is text and easy to inspect but bulky. BAM is the binary compressed form used in almost every pipeline and viewer. CRAM is also binary but reference-based for still smaller size. Full table: [[format-alignments-compared|SAM vs BAM vs CRAM]].",
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — SAM ↔ BAM ↔ CRAM (CRAM needs a [[format-fasta|FASTA]] reference).",
          "Merge — concatenate SAM inputs.",
          "Align — can emit SAM or .sam.gz; for browsing, prefer BAM/CRAM with sort + index.",
          "View — designed around indexed BAM/CRAM random access; convert large SAM to BAM first.",
        ],
      },
      {
        type: "note",
        text: "Unsorted SAM is fine for streaming. Genome browsers and HelixGT View expect coordinate-sorted BAM/CRAM plus .bai/.crai.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: [
          "[[format-bam|BAM]]",
          "[[format-cram|CRAM]]",
          "[[format-alignments-compared|SAM vs BAM vs CRAM]]",
          "[[formats|Supported formats]]",
        ],
      },
    ],
  },
  {
    id: "format-bam",
    categoryId: "formats",
    title: "BAM",
    keywords: ["bam", "bai", "binary", "alignment", "index", "pileup", "bgzf"],
    blocks: [
      {
        type: "p",
        text: "[[format-bam|BAM]] is the binary, block-compressed counterpart to [[format-sam|SAM]]. It stores the same alignment records and headers, but in a form that is smaller, faster for computers to parse, and indexable for jumping to a genomic locus without reading the entire file.",
      },
      {
        type: "h3",
        text: "Why BAM exists",
      },
      {
        type: "p",
        text: "Plain SAM for a whole-genome sample can be enormous and slow. BAM uses BGZF compression (gzip-compatible blocks) so tools can seek into the file. With a coordinate-sorted BAM and a .bai index, [[view|View]] and IGV load only the window you ask for.",
      },
      {
        type: "h3",
        text: "Self-contained sequence",
      },
      {
        type: "p",
        text: "Unlike [[format-cram|CRAM]], BAM normally embeds each read’s bases (and qualities) in the file. You do not need the reference FASTA merely to list alignments—though a matching [[format-fasta|FASTA]] still helps for mismatch highlighting and many analyses.",
      },
      {
        type: "h3",
        text: "Indexing",
      },
      {
        type: "ul",
        items: [
          ".bai — standard BAI index next to sample.bam as sample.bam.bai or sample.bai (tool-dependent naming)",
          "BAM must be coordinate-sorted before indexing for normal genomic browsers",
          "Without an index, random-access viewers cannot open large BAMs efficiently",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".bam", "index: .bai"],
      },
      {
        type: "h3",
        text: "BAM vs SAM vs CRAM",
      },
      {
        type: "p",
        text: "Think of BAM as “SAM for production.” Use SAM when you must read alignments as text; use CRAM when you want smaller files and accept a mandatory reference. Details: [[format-alignments-compared|SAM vs BAM vs CRAM]].",
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — interchange with SAM/CRAM; compression is inherent (no text gzip toggle).",
          "Merge — pure BAM merge for BAM inputs.",
          "Align — common output; enable Sort + Create index for View/IGV.",
          "View — coverage, pileup, read list, filters on sorted+indexed BAM.",
        ],
      },
      {
        type: "tip",
        text: "After Align, turn on “Create index (.bai / .crai)” so the BAM opens immediately in View.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: [
          "[[format-sam|SAM]]",
          "[[format-cram|CRAM]]",
          "[[format-alignments-compared|SAM vs BAM vs CRAM]]",
          "[[view|View mode]]",
        ],
      },
    ],
  },
  {
    id: "format-cram",
    categoryId: "formats",
    title: "CRAM",
    keywords: ["cram", "crai", "reference", "compression", "alignment"],
    blocks: [
      {
        type: "p",
        text: "[[format-cram|CRAM]] is a compressed alignment container related to [[format-sam|SAM]]/[[format-bam|BAM]], designed to shrink large datasets by storing differences relative to a reference [[format-fasta|FASTA]]. The logical records (flags, CIGAR, positions, tags) remain alignment data—you are not changing the science, only the encoding.",
      },
      {
        type: "h3",
        text: "Reference dependence",
      },
      {
        type: "p",
        text: "To encode or decode CRAM you need the same (or a compatible) assembly that was used when the file was written. If the wrong FASTA is supplied, tools may error or reconstruct incorrect bases. Keep the reference path with your project archive.",
      },
      {
        type: "h3",
        text: "Size and trade-offs",
      },
      {
        type: "ul",
        items: [
          "Often smaller than BAM for high-coverage WGS against a good reference",
          "Slightly more tooling friction: always pass the reference; index is .crai",
          "Still requires coordinate sort + index for efficient View/IGV access",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".cram", "index: .crai"],
      },
      {
        type: "h3",
        text: "CRAM vs BAM vs SAM",
      },
      {
        type: "p",
        text: "CRAM ≈ BAM’s role in pipelines, with smaller files and a mandatory reference. SAM remains the text form. Side-by-side: [[format-alignments-compared|SAM vs BAM vs CRAM]]. Operational notes: [[cram-and-reference|CRAM and reference genomes]].",
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — to/from SAM or BAM requires Reference FASTA (samtools path for CRAM I/O).",
          "Merge — header-based concatenation; for full re-encode, convert via BAM.",
          "Align — good default for large runs with sort + .crai.",
          "View — needs .crai and the correct FASTA (Convert reference or Set as reference).",
        ],
      },
      {
        type: "note",
        text: "Mismatched assemblies are the #1 CRAM failure mode—verify build names (e.g. GRCh38) before converting.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: [
          "[[format-bam|BAM]]",
          "[[format-sam|SAM]]",
          "[[format-alignments-compared|SAM vs BAM vs CRAM]]",
          "[[cram-and-reference|CRAM and reference genomes]]",
        ],
      },
    ],
  },
  {
    id: "format-vcf",
    categoryId: "formats",
    title: "VCF",
    keywords: ["vcf", "bcf", "variant", "snp", "indel", "genotype"],
    blocks: [
      {
        type: "p",
        text: "[[format-vcf|VCF]] (Variant Call Format) describes differences relative to a reference: SNPs, insertions, deletions, and structural variants, often with sample genotypes. It is not a read alignment format ([[format-bam|BAM]]) and not a gene annotation track ([[format-gff|GFF]]).",
      },
      {
        type: "h3",
        text: "Structure",
      },
      {
        type: "ul",
        items: [
          "## meta-information lines (fileformat, filters, INFO/FORMAT definitions, reference, contigs)",
          "One #CHROM header line naming fixed columns and sample columns",
          "Data lines: CHROM, POS, ID, REF, ALT, QUAL, FILTER, INFO, then FORMAT and per-sample fields",
        ],
      },
      {
        type: "h3",
        text: "Typical extensions",
      },
      {
        type: "ul",
        items: [".vcf", ".vcf.gz (often bgzip + tabix in broader ecosystems)"],
      },
      {
        type: "h3",
        text: "How it differs from related formats",
      },
      {
        type: "ul",
        items: [
          "[[format-sam|SAM]]/[[format-bam|BAM]]/[[format-cram|CRAM]] — where each read maps; VCF summarizes variant sites (often after calling on BAM).",
          "[[format-gff|GFF]]/[[format-bed|BED]] — arbitrary or gene features; VCF is allele-centric at positions.",
          "[[format-fasta|FASTA]] — the reference sequence variants are called against.",
        ],
      },
      {
        type: "h3",
        text: "In HelixGT",
      },
      {
        type: "ul",
        items: [
          "Convert — text variant handling with optional compression where supported.",
          "Merge — same-type combination (concatenation-style), not a full multi-sample joint callset merge like bcftools merge semantics for complex cohorts.",
          "View — not a dedicated VCF browser; use Convert/Merge for file ops and external tools for deep variant exploration if needed.",
        ],
      },
      {
        type: "tip",
        text: "Keep VCF contig names consistent with your reference FASTA and BAM @SQ lines for any cross-tool workflow.",
      },
      {
        type: "h3",
        text: "See also",
      },
      {
        type: "ul",
        items: ["[[format-fasta|FASTA]]", "[[format-bam|BAM]]", "[[formats|Supported formats]]"],
      },
    ],
  },

  // ── References & CRAM ────────────────────────────────────────────
  {
    id: "cram-and-reference",
    categoryId: "ref",
    title: "CRAM and reference genomes",
    keywords: ["cram", "reference", "assembly", "crai"],
    blocks: [
      {
        type: "p",
        text: "[[format-cram|CRAM]] is a reference-based alignment container. Encoding and decoding need the same (or compatible) [[format-fasta|FASTA]] used at creation time. HelixGT uses samtools for CRAM I/O when converting or viewing. For a format comparison with SAM and BAM, see [[format-alignments-compared|SAM vs BAM vs CRAM]].",
      },
      {
        type: "h3",
        text: "Where to set a reference",
      },
      {
        type: "ul",
        items: [
          "Convert pane — when output or inputs involve CRAM.",
          "Align pane — reference download or local FASTA for mapping (also usable as CRAM reference).",
          "Input browser — right-click FASTA → Set as reference.",
        ],
      },
      {
        type: "h3",
        text: "NCBI downloads",
      },
      {
        type: "p",
        text: "Align can download selected RefSeq assemblies into a local cache. Human-scale genomes are hundreds of megabytes compressed and take time; keep the connection stable.",
      },
    ],
  },
  {
    id: "external-tools",
    categoryId: "ref",
    title: "minimap2 and samtools",
    keywords: ["minimap2", "samtools", "binaries", "path", "dll"],
    blocks: [
      {
        type: "p",
        text: "Alignment and CRAM workflows call minimap2 and samtools. Release builds ship pre-built Windows binaries (and required DLLs) with the app. Development builds also look on the system PATH if local binaries are missing.",
      },
      {
        type: "ul",
        items: [
          "If Align shows tools unavailable, check that binaries are present for your build.",
          "Tool stdout/stderr streams appear under Logs → Tools during jobs.",
          "Pure text convert/merge and BAM-only paths often run without spawning external processes.",
        ],
      },
    ],
  },

  // ── Logs ─────────────────────────────────────────────────────────
  {
    id: "logs",
    categoryId: "logs",
    title: "Logs drawer",
    keywords: ["logs", "activity", "tools", "errors", "export"],
    blocks: [
      {
        type: "p",
        text: "Open Logs from the top menu bar (right side). The drawer has two tabs:",
      },
      {
        type: "ul",
        items: [
          "Activity — app messages: job start/finish, preflight, QC summaries, warnings, errors.",
          "Tools — line-oriented output from minimap2 and samtools during external jobs.",
        ],
      },
      {
        type: "h3",
        text: "Toolbar",
      },
      {
        type: "ul",
        items: [
          "Filter box — substring match on message text.",
          "Level filter — all / info / warn / error (Activity).",
          "Auto-scroll — keep the latest line in view.",
          "Copy / Export / Clear — clipboard, download a .txt file, or clear the current tab.",
        ],
      },
      {
        type: "tip",
        text: "When a job emits a new error, Logs may open automatically so failures are not missed.",
      },
    ],
  },

  // ── Reference sheet ──────────────────────────────────────────────
  {
    id: "shortcuts",
    categoryId: "ref-sheet",
    title: "Keyboard & mouse shortcuts",
    keywords: ["shortcuts", "keyboard", "mouse", "scroll", "escape"],
    blocks: [
      {
        type: "h3",
        text: "View mode",
      },
      {
        type: "kbd",
        items: [
          { keys: "Scroll", desc: "Pan along the contig" },
          { keys: "Ctrl + Scroll", desc: "Zoom in / out" },
          { keys: "Shift-drag or middle-drag", desc: "Pan the canvas" },
          { keys: "Drag", desc: "Select a base range" },
          { keys: "Click", desc: "Inspect feature, read, or coverage bar" },
          { keys: "Double-click", desc: "Zoom to feature or read" },
        ],
      },
      {
        type: "h3",
        text: "Global",
      },
      {
        type: "kbd",
        items: [
          { keys: "Esc", desc: "Close Logs drawer or User manual" },
        ],
      },
    ],
  },
  {
    id: "troubleshooting",
    categoryId: "ref-sheet",
    title: "Troubleshooting",
    keywords: ["error", "fix", "help", "failed", "missing index"],
    blocks: [
      {
        type: "h3",
        text: "CRAM convert fails",
      },
      {
        type: "ul",
        items: [
          "Set the correct reference FASTA for the assembly.",
          "Confirm samtools is available (bundled binaries present).",
          "Check Logs → Tools for the underlying samtools message.",
        ],
      },
      {
        type: "h3",
        text: "View shows no reads",
      },
      {
        type: "ul",
        items: [
          "BAM/CRAM must be coordinate-sorted and indexed (.bai / .crai beside the file).",
          "Contig names must match the open reference (chr1 vs 1).",
          "Relax filters (secondary, MAPQ) or zoom to a region that has coverage.",
          "For CRAM, set the reference FASTA used at alignment time.",
        ],
      },
      {
        type: "h3",
        text: "Align cannot start",
      },
      {
        type: "ul",
        items: [
          "Load a reference (download or local FASTA).",
          "Select FASTA/FASTQ reads in the browser.",
          "Ensure minimap2 and samtools are available.",
          "Pick an output folder with write permission and free disk space.",
        ],
      },
      {
        type: "h3",
        text: "Slow UI in View",
      },
      {
        type: "ul",
        items: [
          "Zoom in: very wide windows load more depth and reads.",
          "Hide pileup or read list when you only need coverage.",
          "Filter secondaries and low MAPQ to reduce glyphs.",
        ],
      },
      {
        type: "h3",
        text: "Limitations",
      },
      {
        type: "ul",
        items: [
          "FASTA → GenBank is not supported.",
          "Huge reference downloads and multi-hour alignments need patience and disk space.",
          "View loads a windowed slice of data, not entire multi-GB BAMs into memory at once.",
        ],
      },
    ],
  },
  {
    id: "about",
    categoryId: "ref-sheet",
    title: "About HelixGT",
    keywords: ["about", "license", "tauri", "rust", "svelte"],
    blocks: [
      {
        type: "p",
        text: "HelixGT — Genomics Toolkit. Convert, merge, align, and browse genomics files on Windows.",
      },
      {
        type: "ul",
        items: [
          "UI: Svelte + Tauri 2",
          "Engine: Rust (converter-core) with noodles and related crates",
          "External: minimap2 and samtools for alignment and CRAM",
        ],
      },
      {
        type: "p",
        text: "Formats: FASTA, FASTQ, GenBank, GFF, BED, SAM, BAM, CRAM, VCF.",
      },
    ],
  },
];

const pageById = new Map(MANUAL_PAGES.map((p) => [p.id, p]));

export function getManualPage(id: string): ManualPage | undefined {
  return pageById.get(id);
}

export function getDefaultManualPageId(): string {
  return "welcome";
}

export function searchManualPages(query: string): ManualPage[] {
  const q = query.trim().toLowerCase();
  if (!q) return MANUAL_PAGES;
  return MANUAL_PAGES.filter((page) => {
    if (page.title.toLowerCase().includes(q)) return true;
    if (page.keywords.some((k) => k.includes(q))) return true;
    return page.blocks.some((block) => {
      if (block.type === "p" || block.type === "h3" || block.type === "note" || block.type === "tip") {
        return plainWikiText(block.text).toLowerCase().includes(q);
      }
      if (block.type === "ul" || block.type === "ol") {
        return block.items.some((item) => plainWikiText(item).toLowerCase().includes(q));
      }
      if (block.type === "table") {
        return (
          block.headers.some((h) => plainWikiText(h).toLowerCase().includes(q)) ||
          block.rows.some((row) =>
            row.some((cell) => plainWikiText(cell).toLowerCase().includes(q)),
          )
        );
      }
      if (block.type === "kbd") {
        return block.items.some(
          (item) =>
            plainWikiText(item.keys).toLowerCase().includes(q) ||
            plainWikiText(item.desc).toLowerCase().includes(q),
        );
      }
      return false;
    });
  });
}

export function pagesInCategory(categoryId: string): ManualPage[] {
  return MANUAL_PAGES.filter((p) => p.categoryId === categoryId);
}
