use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pactole_core::DiagnosticCode;
use pactole_storage_fs::{AnalyzedProject, Diagnostic, FsSourceLoader, normalize_lexically};

use crate::commands::GraphFormatArg;

#[derive(Debug)]
/// One include edge in the analyzed project graph.
struct Edge {
    /// File containing the include directive.
    from: PathBuf,
    /// Resolved target, when the include has a usable base directory.
    to: Option<PathBuf>,
    /// Include path as written in the source.
    include: String,
    /// One-based source line of the include directive.
    line: usize,
}

/// Analyzes and renders the include graph for a journal.
pub(crate) fn run(file: PathBuf, scan: Option<PathBuf>, format: GraphFormatArg) -> ExitCode {
    let analysis_path = match absolute_lexically(&file) {
        Ok(path) => path,
        Err(error) => return fail(format, format!("{}: {error}", file.display())),
    };
    let project =
        match pactole_storage_fs::analyze_file_with_loader(&analysis_path, &FsSourceLoader) {
            Ok(project) => project,
            Err(error) => {
                return fail(format, format!("{}: {error}", file.display()));
            }
        };

    let scan_root = match scan.as_deref().map(absolute_lexically).transpose() {
        Ok(scan_root) => scan_root,
        Err(error) => return fail(format, error.to_string()),
    };
    let scanned = match scan_root.as_deref().map(find_pactole_files).transpose() {
        Ok(Some(files)) => files,
        Ok(None) => BTreeSet::new(),
        Err(error) => return fail(format, error),
    };

    let edges = edges(&project);
    let reachable: BTreeSet<_> = project
        .files()
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let unreachable: BTreeSet<_> = scanned.difference(&reachable).cloned().collect();
    let diagnostics: Vec<_> = project
        .diagnostics()
        .into_iter()
        .filter(|diagnostic| diagnostic.source == pactole_storage_fs::DiagnosticSource::Include)
        .collect();

    match format {
        GraphFormatArg::Text => print_text(&file, &edges, &reachable, &unreachable, &diagnostics),
        GraphFormatArg::Json => print_json(
            &file,
            &edges,
            &reachable,
            &unreachable,
            &scanned,
            &diagnostics,
        ),
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == pactole_storage_fs::DiagnosticSeverity::Error)
    {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Makes a path absolute and normalizes its lexical components.
fn absolute_lexically(path: &Path) -> std::io::Result<PathBuf> {
    std::path::absolute(path).map(|absolute| normalize_lexically(&absolute))
}

/// Extracts and sorts include edges from an analyzed project.
fn edges(project: &AnalyzedProject) -> Vec<Edge> {
    let mut edges = Vec::new();
    for file in project.files() {
        for include in file.file.includes() {
            let raw = PathBuf::from(&include.path);
            let target = if raw.is_absolute() {
                Some(normalize_lexically(&raw))
            } else {
                file.path
                    .parent()
                    .map(|directory| normalize_lexically(&directory.join(&raw)))
            };
            edges.push(Edge {
                from: file.path.clone(),
                to: target,
                include: include.path.clone(),
                line: include.span.start.row + 1,
            });
        }
    }
    edges.sort_by(|left, right| {
        (&left.from, &left.to, &left.include, left.line).cmp(&(
            &right.from,
            &right.to,
            &right.include,
            right.line,
        ))
    });
    edges
}

/// Renders the graph and include diagnostics as human-readable text.
fn print_text(
    root: &Path,
    edges: &[Edge],
    reachable: &BTreeSet<PathBuf>,
    unreachable: &BTreeSet<PathBuf>,
    diagnostics: &[Diagnostic],
) {
    println!("Include graph: {}", root.display());
    if edges.is_empty() {
        println!("  (no includes)");
    }

    let mut target_counts = BTreeMap::<PathBuf, usize>::new();
    for edge in edges {
        if let Some(target) = &edge.to {
            *target_counts.entry(target.clone()).or_default() += 1;
        }
    }
    for edge in edges {
        match &edge.to {
            Some(target) => {
                let repeated = target_counts.get(target).copied().unwrap_or_default() > 1;
                let state = if reachable.contains(target) {
                    "reachable"
                } else {
                    "unresolved"
                };
                let repeated_label = if repeated {
                    " [included more than once]"
                } else {
                    ""
                };
                println!(
                    "  {}:{} -> {} ({state}){repeated_label}",
                    edge.from.display(),
                    edge.line,
                    target.display()
                );
            }
            None => println!(
                "  {}:{} -> {} (unresolved: no base directory)",
                edge.from.display(),
                edge.line,
                edge.include
            ),
        }
    }

    if !unreachable.is_empty() {
        println!("Unreachable .pactole files:");
        for path in unreachable {
            println!("  {}", path.display());
        }
    }

    if !diagnostics.is_empty() {
        println!("Include diagnostics:");
        for diagnostic in diagnostics {
            println!(
                "  {}:{} [{}] {}",
                diagnostic.path.display(),
                diagnostic.span.start.row + 1,
                diagnostic.code,
                diagnostic.message
            );
        }
    }
}

/// Renders the graph and include diagnostics as JSON.
fn print_json(
    root: &Path,
    edges: &[Edge],
    reachable: &BTreeSet<PathBuf>,
    unreachable: &BTreeSet<PathBuf>,
    scanned: &BTreeSet<PathBuf>,
    diagnostics: &[Diagnostic],
) {
    let mut all_files = reachable.clone();
    all_files.extend(scanned.iter().cloned());
    for edge in edges {
        if let Some(target) = &edge.to {
            all_files.insert(target.clone());
        }
    }

    let mut target_counts = BTreeMap::<PathBuf, usize>::new();
    for edge in edges {
        if let Some(target) = &edge.to {
            *target_counts.entry(target.clone()).or_default() += 1;
        }
    }
    let files: Vec<_> = all_files
        .iter()
        .map(|path| {
            let state = if reachable.contains(path) {
                "reachable"
            } else if unreachable.contains(path) {
                "unreachable"
            } else {
                "unresolved"
            };
            serde_json::json!({
                "path": path.display().to_string(),
                "state": state,
                "include_count": target_counts.get(path).copied().unwrap_or_default(),
            })
        })
        .collect();
    let edges_json: Vec<_> = edges
        .iter()
        .map(|edge| {
            let count = edge
                .to
                .as_ref()
                .and_then(|target| target_counts.get(target))
                .copied()
                .unwrap_or_default();
            serde_json::json!({
                "from": edge.from.display().to_string(),
                "to": edge.to.as_ref().map(|path| path.display().to_string()),
                "include": edge.include,
                "line": edge.line,
                "repeated_target": count > 1,
            })
        })
        .collect();
    let diagnostics_json: Vec<_> = diagnostics
        .iter()
        .map(crate::check::diagnostic_json)
        .collect();
    let cycles: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::IncludeCycle)
        .map(crate::check::diagnostic_json)
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": !diagnostics.iter().any(|diagnostic| diagnostic.severity == pactole_storage_fs::DiagnosticSeverity::Error),
            "root": root.display().to_string(),
            "files": files,
            "edges": edges_json,
            "unreachable": unreachable.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "cycles": cycles,
            "diagnostics": diagnostics_json,
        }))
        .unwrap_or_else(|_| String::from("{}"))
    );
}

/// Recursively finds normalized `.pactole` files below a directory.
fn find_pactole_files(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("cannot scan {}: {error}", directory.display()))?;
        let mut paths = entries
            .map(|entry| entry.map(|entry| (entry.path(), entry.file_type())))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot scan {}: {error}", directory.display()))?;
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        for (path, file_type) in paths.into_iter().rev() {
            let file_type =
                file_type.map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("pactole")
            {
                found.insert(normalize_lexically(&path));
            }
        }
    }
    Ok(found)
}

/// Renders a graph error in the requested format.
fn fail(format: GraphFormatArg, message: String) -> ExitCode {
    match format {
        GraphFormatArg::Json => println!("{}", serde_json::json!({"ok": false, "error": message})),
        GraphFormatArg::Text => eprintln!("error: {message}"),
    }
    ExitCode::FAILURE
}
