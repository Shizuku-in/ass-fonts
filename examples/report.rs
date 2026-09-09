use ass_fonts::{FontIndex, ScanReport, read_subtitle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let subtitle = args
        .next()
        .ok_or("usage: report <subtitle.ass|ssa> <font file/directory>...")?;
    let roots: Vec<_> = args.collect();
    if roots.is_empty() {
        return Err("provide at least one font file/directory".into());
    }
    let extracted = read_subtitle(subtitle)?;
    let scanned = ScanReport::scan(roots);
    let report = FontIndex::new(scanned.faces).resolve(&extracted.references);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "resolved": report.resolved,
            "missing": report.missing,
            "ambiguous": report.ambiguous,
            "subtitle_diagnostics": extracted.diagnostics,
            "scan_issues": scanned.issues,
        }))?
    );
    Ok(())
}
