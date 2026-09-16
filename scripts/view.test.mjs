import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import ts from "typescript";

// Exercise the real TS helpers using the project's existing compiler, without
// requiring a browser, Tauri, generated build files, or another test dependency.
function moduleUrl(relativePath) {
  const url = new URL(relativePath, import.meta.url);
  const source = ts.transpileModule(readFileSync(url, "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText.replace(/from "(\.\/[^".]+)"/g, (_, specifier) =>
    `from "${moduleUrl(new URL(`${specifier}.ts`, url).href)}"`);
  return `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
}
const fetch = await import(moduleUrl("../src/lib/view/fetch.ts"));
const { parseLocus } = await import(moduleUrl("../src/lib/view/locus.ts"));
const { packReadsSquish, readsOverlapWindow } = await import(moduleUrl("../src/lib/view/reads.ts"));

test("coverage planning contains the entire viewport at every zoom", () => {
  for (const span of [20, 1500, 12000, 50000, 2000000, 250000000]) {
    const viewStart = 1234;
    const viewEnd = viewStart + span;
    const plan = fetch.computeCoverageFetchWindow(viewStart, viewEnd, 300000000, span);
    assert.ok(plan.start <= viewStart && plan.end >= viewEnd);
    const binCount = fetch.desiredCoverageBinCount(plan.end - plan.start, 800, span);
    assert.ok(binCount <= 4096 && binCount <= plan.end - plan.start);
    assert.ok(fetch.coverageCacheDenseEnough({ ...plan, binCount }, viewStart, viewEnd, 800));
  }
});

test("read cache can display bars while requesting missing base detail", () => {
  const cache = { path: "x.bam", contig: "chr1", start: 0, end: 10000,
    filterKey: "0|noseq", truncated: false, hasSequences: false };
  assert.equal(fetch.readCacheCanDisplay(cache, "x.bam", "chr1", "0|seq", 100, 500), true);
  assert.equal(fetch.readCacheUsable(cache, "x.bam", "chr1", "0|seq", 100, 500, true), false);
  assert.equal(fetch.readCacheCanDisplay(cache, "x.bam", "chr2", "0|seq", 100, 500), false);
  assert.equal(fetch.readCacheCanDisplay(cache, "x.bam", "chr1", "1|seq", 100, 500), false);
});

test("long reads starting before the viewport are retained", () => {
  const reads = [{ start: 0, end: 10000 }, { start: 10, end: 20 }, { start: 1000, end: 1100 }];
  assert.deepEqual(readsOverlapWindow(reads, 500, 600), [reads[0]]);
  assert.deepEqual(readsOverlapWindow(reads, 10000, 10001), []);
});

test("pileup accounts for every read omitted by lane and input caps", () => {
  const reads = Array.from({ length: 6500 }, (_, i) => ({ name: `r${i}`, start: i, end: i + 10 }));
  const result = packReadsSquish(reads, 0, 7000, 3);
  assert.equal(result.packed.length + result.hiddenCount, reads.length);
  assert.ok(result.laneCount <= 3);
});

test("locus coordinates are integral and cannot invert an open-ended range", () => {
  for (const text of ["chr1:1.5", "chr1:1001-", "chr1:1001", "chr1:1-2.5", "chr1:9007199254740992"]) {
    assert.equal(parseLocus(text, "chr1", 1000).ok, false, text);
  }
  assert.deepEqual(parseLocus("chr2:900-", "chr2", 1000),
    { ok: true, contig: "chr2", viewStart: 899, viewEnd: 1000 });
  assert.deepEqual(parseLocus("chr2:900-2,000", "chr2", 1000),
    { ok: true, contig: "chr2", viewStart: 899, viewEnd: 1000 });
});
