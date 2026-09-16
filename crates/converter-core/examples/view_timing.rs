use std::{path::Path, time::Instant};
use converter_core::{get_coverage_bins_filtered, get_reads_in_range_filtered, ReadQueryOptions, ToolPaths};
fn main() -> anyhow::Result<()> {
    std::thread::Builder::new().stack_size(8 * 1024 * 1024).spawn(run)?.join().unwrap()
}
fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(args.len() >= 3, "usage: view_timing ALIGNMENT REFERENCE [CONTIG] [START] [END]");
    let contig = args.get(3).map(String::as_str).unwrap_or("chr1");
    let start = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let end = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(125_000);
    let tools = ToolPaths { samtools: Some(Path::new("src-tauri/binaries/samtools.exe").into()), minimap2: None };
    for attempt in 1..=2 {
        let t = Instant::now();
        eprintln!("coverage attempt {attempt}: {contig}:{start}-{end}");
        if args.get(6).is_some_and(|s| s == "reads") {
            let options = ReadQueryOptions { include_sequences: true, ..Default::default() };
            let result = get_reads_in_range_filtered(Path::new(&args[1]), &tools, contig, start, end,
                Some(Path::new(&args[2])), &options)?;
            eprintln!("elapsed={:.3}s reads={} total={}", t.elapsed().as_secs_f64(), result.reads.len(), result.total_in_range);
            continue;
        }
        let result = get_coverage_bins_filtered(Path::new(&args[1]), &tools, contig, start, end, 2400,
            Some(Path::new(&args[2])), &ReadQueryOptions::default())?;
        eprintln!("elapsed={:.3}s bins={} max_depth={:.3}", t.elapsed().as_secs_f64(), result.bins.len(), result.max_depth);
    }
    Ok(())
}
