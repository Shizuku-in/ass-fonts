use ass_fonts::{
    CoverageReport, CoverageSummary, FontIndex, ResolveOptions, ScanReport, WeightMatching,
    read_subtitle,
};
use std::{error::Error, fmt, process::ExitCode};

const USAGE: &str = "usage: report [--nearest-weight] [--allow-style-synthesis] \
    [--check-coverage] [--summary] [--fail-on-missing-font] \
    [--fail-on-ambiguous-font] [--fail-on-missing-glyph] [--fail-on-uncheckable] \
    [--] <subtitle.ass|ssa> <font file/directory>...";

#[derive(Default)]
struct CliOptions {
    resolve: ResolveOptions,
    check_coverage: bool,
    summary: bool,
    fail_on_missing_font: bool,
    fail_on_ambiguous_font: bool,
    fail_on_missing_glyph: bool,
    fail_on_uncheckable: bool,
}

#[derive(Debug)]
struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for UsageError {}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(if error.is::<UsageError>() { 2 } else { 1 })
        }
    }
}

fn run() -> Result<u8, Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1).peekable();
    let mut cli = CliOptions::default();
    while let Some(arg) = args.peek() {
        if arg == "--allow-style-synthesis" {
            cli.resolve.synthesis.bold = true;
            cli.resolve.synthesis.italic = true;
        } else if arg == "--nearest-weight" {
            cli.resolve.weight_matching = WeightMatching::Nearest;
        } else if arg == "--check-coverage" {
            cli.check_coverage = true;
        } else if arg == "--summary" {
            cli.summary = true;
        } else if arg == "--fail-on-missing-font" {
            cli.fail_on_missing_font = true;
        } else if arg == "--fail-on-ambiguous-font" {
            cli.fail_on_ambiguous_font = true;
        } else if arg == "--fail-on-missing-glyph" {
            cli.fail_on_missing_glyph = true;
        } else if arg == "--fail-on-uncheckable" {
            cli.fail_on_uncheckable = true;
        } else if arg == "--" {
            args.next();
            break;
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(Box::new(UsageError(format!(
                "unknown option: {}\n{USAGE}",
                arg.to_string_lossy()
            ))));
        } else {
            break;
        }
        args.next();
    }
    let subtitle = args
        .next()
        .ok_or_else(|| Box::new(UsageError(USAGE.into())) as Box<dyn Error>)?;
    let roots: Vec<_> = args.collect();
    if roots.is_empty() {
        return Err(Box::new(UsageError(format!(
            "provide at least one font file/directory\n{USAGE}"
        ))));
    }

    let extracted = read_subtitle(subtitle)?;
    let scanned = ScanReport::scan(roots);
    let report =
        FontIndex::new(scanned.faces).resolve_with_options(&extracted.references, cli.resolve);
    let coverage_requested =
        cli.check_coverage || cli.summary || cli.fail_on_missing_glyph || cli.fail_on_uncheckable;
    let coverage = coverage_requested.then(|| report.check_coverage());
    let coverage_summary = coverage.as_ref().map(CoverageReport::summary);
    let exit_code = failure_exit_code(
        &cli,
        report.missing.len(),
        report.ambiguous.len(),
        coverage_summary,
    );

    let output = if cli.summary {
        let coverage = coverage.as_ref().expect("summary requests coverage");
        let missing_characters = coverage
            .missing_characters()
            .into_iter()
            .map(|usage| {
                serde_json::json!({
                    "character": usage.character,
                    "codepoint": usage.codepoint(),
                    "lines": usage.lines,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "options": cli.resolve,
            "font_references": extracted.references.len(),
            "resolution": {
                "resolved": report.resolved.len(),
                "missing": report.missing.len(),
                "ambiguous": report.ambiguous.len(),
            },
            "coverage": {
                "summary": coverage_summary.expect("summary requests coverage"),
                "missing_characters": missing_characters,
            },
            "subtitle_diagnostics": extracted.diagnostics.len(),
            "scan_issues": scanned.issues.len(),
        })
    } else {
        let mut output = serde_json::json!({
            "options": cli.resolve,
            "resolved": report.resolved,
            "missing": report.missing,
            "ambiguous": report.ambiguous,
            "subtitle_diagnostics": extracted.diagnostics,
            "scan_issues": scanned.issues,
        });
        if let Some(coverage) = coverage {
            output["coverage"] = serde_json::to_value(coverage)?;
        }
        output
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(exit_code)
}

fn failure_exit_code(
    cli: &CliOptions,
    missing_fonts: usize,
    ambiguous_fonts: usize,
    coverage: Option<CoverageSummary>,
) -> u8 {
    if (cli.fail_on_missing_font && missing_fonts > 0)
        || (cli.fail_on_ambiguous_font && ambiguous_fonts > 0)
    {
        return 3;
    }
    if cli.fail_on_missing_glyph && coverage.is_some_and(|value| value.incomplete_candidates > 0) {
        return 4;
    }
    if cli.fail_on_uncheckable
        && coverage.is_some_and(|value| {
            value.uncheckable_candidates > 0 || value.unresolved_references > 0
        })
    {
        return 5;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_policies_are_opt_in_and_have_stable_precedence() {
        let mut cli = CliOptions::default();
        let coverage = CoverageSummary {
            incomplete_candidates: 1,
            uncheckable_candidates: 1,
            ..Default::default()
        };
        assert_eq!(failure_exit_code(&cli, 1, 1, Some(coverage)), 0);
        cli.fail_on_uncheckable = true;
        assert_eq!(failure_exit_code(&cli, 0, 0, Some(coverage)), 5);
        cli.fail_on_missing_glyph = true;
        assert_eq!(failure_exit_code(&cli, 0, 0, Some(coverage)), 4);
        cli.fail_on_missing_font = true;
        assert_eq!(failure_exit_code(&cli, 1, 0, Some(coverage)), 3);
        cli.fail_on_missing_font = false;
        cli.fail_on_ambiguous_font = true;
        assert_eq!(failure_exit_code(&cli, 0, 1, Some(coverage)), 3);
    }
}
