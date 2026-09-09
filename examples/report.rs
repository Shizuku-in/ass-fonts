use ass_fonts::{FontIndex, ResolveOptions, ScanReport, WeightMatching, read_subtitle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1).peekable();
    let mut options = ResolveOptions::default();
    while let Some(arg) = args.peek() {
        if arg == "--allow-style-synthesis" {
            options.synthesis.bold = true;
        } else if arg == "--nearest-weight" {
            options.weight_matching = WeightMatching::Nearest;
        } else if arg == "--" {
            args.next();
            break;
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}", arg.to_string_lossy()).into());
        } else {
            break;
        }
        args.next();
    }
    let subtitle = args.next().ok_or(
        "usage: report [--nearest-weight] [--allow-style-synthesis] [--] <subtitle.ass|ssa> <font file/directory>...",
    )?;
    let roots: Vec<_> = args.collect();
    if roots.is_empty() {
        return Err("provide at least one font file/directory".into());
    }
    let extracted = read_subtitle(subtitle)?;
    let scanned = ScanReport::scan(roots);
    let report = FontIndex::new(scanned.faces).resolve_with_options(&extracted.references, options);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "options": options,
            "resolved": report.resolved,
            "missing": report.missing,
            "ambiguous": report.ambiguous,
            "subtitle_diagnostics": extracted.diagnostics,
            "scan_issues": scanned.issues,
        }))?
    );
    Ok(())
}
