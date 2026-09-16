/** Shared contig-name aliases (chr1 ↔ 1, chrM ↔ MT). */

export function contigNameAliases(name: string): string[] {
  const out = [name];
  const lower = name.toLowerCase();
  if (lower.startsWith("chr") && name.length > 3) {
    const bare = name.slice(3);
    out.push(bare);
    if (bare.toLowerCase() === "m" || bare.toLowerCase() === "mt") {
      out.push("MT", "M", "chrM", "chrMT");
    }
  } else {
    out.push(`chr${name}`);
    if (lower === "mt" || lower === "m") {
      out.push("chrM", "chrMT", "MT", "M");
    }
  }
  return out;
}

export function resolveContigName(names: string[], requested: string): string | null {
  if (!requested) return null;
  for (const alias of contigNameAliases(requested)) {
    const hit = names.find((n) => n === alias);
    if (hit) return hit;
  }
  return names.find((n) => n.toLowerCase() === requested.toLowerCase()) ?? null;
}

export function findContigLength(
  contigs: { name: string; length: number }[] | undefined,
  name: string,
): number | null {
  if (!contigs?.length || !name) return null;
  const resolved = resolveContigName(
    contigs.map((c) => c.name),
    name,
  );
  if (!resolved) return null;
  const hit = contigs.find((c) => c.name === resolved);
  return hit && hit.length > 0 ? hit.length : null;
}

export function featureTypesForFilter(
  filter: "all" | "genes" | "exons" | "cds",
): string[] | null {
  switch (filter) {
    case "genes":
      return ["gene", "mRNA", "transcript", "lnc_RNA", "ncRNA", "pseudogene"];
    case "exons":
      return [
        "exon",
        "CDS",
        "UTR",
        "five_prime_UTR",
        "three_prime_UTR",
        "five_prime_utr",
        "three_prime_utr",
      ];
    case "cds":
      return ["CDS"];
    default:
      return null;
  }
}
