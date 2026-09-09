use ass_fonts::{FontIndex, ResolveMode, ScanReport, read_subtitle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1).peekable();
    let mode = if args
        .peek()
        .is_some_and(|arg| arg == "--allow-style-synthesis")
    {
        args.next();
        ResolveMode::AllowStyleSynthesis
    } else {
        ResolveMode::Strict
    };
    let subtitle = args.next().ok_or(
        "usage: report [--allow-style-synthesis] <subtitle.ass|ssa> <font file/directory>...",
    )?;
    let roots: Vec<_> = args.collect();
    if roots.is_empty() {
        return Err("provide at least one font file/directory".into());
    }
    let extracted = read_subtitle(subtitle)?;
    let scanned = ScanReport::scan(roots);
    let report = FontIndex::new(scanned.faces).resolve_with_mode(&extracted.references, mode);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "mode": mode,
            "resolved": report.resolved,
            "missing": report.missing,
            "ambiguous": report.ambiguous,
            "subtitle_diagnostics": extracted.diagnostics,
            "scan_issues": scanned.issues,
        }))?
    );
    Ok(())
}
