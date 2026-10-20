//! Goto-definition and find-references support for `.pactole` files.
//!
//! Both requests resolve against a single, shared occurrence index built
//! directly from each file's concrete syntax tree (CST): every account,
//! payee, commodity and `include` path token becomes an occurrence tagged
//! with its kind, its name, and whether it is a *definition* (the `open` of
//! an account, a `payee` directive, a `commodity` directive) or a *use* (a
//! posting account, a transaction payee, a balance commodity, ...).
//!
//! Working from the CST rather than the lowered domain model keeps token
//! positions exact (the domain model deliberately carries no source
//! information) and covers constructs the `EntrySpans` API does not expose,
//! such as the commodity restriction list of an `open` directive or the path
//! token of an `include`.
//!
//! When a `journal_file` is configured, the index spans its whole include
//! graph (through [`crate::documents::DocumentsSourceLoader`], so unsaved
//! editor buffers take precedence over what is on disk); with no
//! `journal_file`, the current document is its own entry point, so its own
//! `include`s are followed too. Following the roadmap, a position with no
//! symbol — or a symbol with no declaration — yields an empty result, never
//! an error.

use std::error::Error;
use std::path::{Path, PathBuf};

use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, Range, ReferenceParams, Url,
};

use pactole_storage_fs::{AnalyzedProject, ParsedFile, normalize_lexically};
use pactole_syntax::Span;
use tree_sitter::Node;

use crate::config::Config;
use crate::conversion::{position_to_offset, span_to_range};
use crate::documents::{Documents, analyze_scope, uri_to_path};

/// The category of symbol goto-definition and find-references resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    /// An account name, defined by its `open` directive.
    Account,
    /// A payee name, defined by a `payee` directive.
    Payee,
    /// A commodity name, defined by a `commodity` directive.
    Commodity,
    /// The target file of an `include` directive; the "definition" is the
    /// included file itself.
    IncludePath,
}

/// The symbol found under the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Symbol {
    kind: SymbolKind,
    /// The name as written in the source, without the surrounding quotes of
    /// a payee string or include path.
    name: String,
}

/// One occurrence of a symbol in a file, as extracted from the CST. The
/// name borrows from the file's source text, so indexing a whole project
/// allocates no per-occurrence strings.
#[derive(Debug, Clone)]
struct Occurrence<'a> {
    kind: SymbolKind,
    /// The name as written in the source (payees and include paths without
    /// their surrounding quotes).
    name: &'a str,
    /// The span of the token itself.
    span: Span,
    /// Whether this occurrence *defines* the symbol (an `open` for accounts,
    /// a `payee` directive, a `commodity` directive).
    is_definition: bool,
}

/// An [`Occurrence`] together with the file it was found in, ready to be
/// turned into an LSP [`Location`].
#[derive(Debug, Clone)]
struct ScopedOccurrence<'a> {
    /// The location of the token, or `None` for a file with no URI (an
    /// untitled buffer), which can still take part in matching but cannot be
    /// sent back to the editor.
    location: Option<Location>,
    /// The filesystem path of the containing file, when known.
    path: Option<PathBuf>,
    kind: SymbolKind,
    name: &'a str,
    is_definition: bool,
}

/// One analyzed file of the navigation scope.
struct ScopeEntry<'a> {
    /// The filesystem path of the file, when known.
    path: Option<PathBuf>,
    /// The URI to report for occurrences in this file. For the current
    /// document this is the request's own URI, echoed back so the editor
    /// matches the buffer it already has open.
    uri: Option<Url>,
    file: &'a ParsedFile,
}

/// The set of files navigation searches: every file reachable from the root
/// journal through `include`s, plus the current document when it is not part
/// of that project.
struct Scope<'a> {
    entries: Vec<ScopeEntry<'a>>,
}

impl<'a> Scope<'a> {
    /// Builds the navigation scope from the analyzed project, falling back to
    /// the current document alone when the document is outside the project
    /// (or no project could be analyzed).
    fn build(
        project: Option<&'a AnalyzedProject>,
        current_uri: &Url,
        current_file: &'a ParsedFile,
    ) -> Self {
        // Project paths are normalized lexically (`normalize_lexically`); do
        // the same to the request's own path so a URI containing `..` (or any
        // other non-normalized spelling) still matches its project entry
        // instead of duplicating the same file twice in scope.
        let current_path = uri_to_path(current_uri).map(|path| normalize_lexically(&path));
        let mut entries: Vec<ScopeEntry<'a>> = Vec::new();
        let mut current_covered = false;

        if let Some(project) = project {
            for file in project.files() {
                let is_current = current_path.as_deref() == Some(file.path.as_path());
                current_covered |= is_current;
                entries.push(ScopeEntry {
                    path: Some(file.path.clone()),
                    uri: if is_current {
                        Some(current_uri.clone())
                    } else {
                        Url::from_file_path(&file.path).ok()
                    },
                    file: &file.file,
                });
            }
        }

        if !current_covered {
            entries.push(ScopeEntry {
                path: current_path,
                uri: Some(current_uri.clone()),
                file: current_file,
            });
        }

        Self { entries }
    }
}

/// Resolves an `include` target the same way project analysis does: the raw
/// path is joined to the including file's directory, then normalized
/// lexically (so the result compares equal to `AnalyzedFile::path`).
fn resolve_include(base_dir: Option<&Path>, raw: &str) -> PathBuf {
    let base = base_dir.unwrap_or_else(|| Path::new(""));
    normalize_lexically(&base.join(raw))
}

/// Finds the symbol under `offset` in `file`, by locating the smallest CST
/// token containing it and classifying it from its node kind and context.
fn symbol_in_file(file: &ParsedFile, offset: usize) -> Option<Symbol> {
    let source = file.document().source();
    let root = file.document().root_node();
    let offset = offset.min(source.len());

    // Editors commonly place the cursor right *after* a token (e.g. at the
    // end of a word); the token's half-open span then no longer contains the
    // offset, so retry with the byte just before it.
    let node = node_at_offset(root, offset);
    if let Some(symbol) = node.and_then(|node| classify_node(node, source)) {
        return Some(symbol);
    }
    offset
        .checked_sub(1)
        .and_then(|o| node_at_offset(root, o))
        .and_then(|node| classify_node(node, source))
}

/// The smallest CST node spanning `offset`, if any.
fn node_at_offset(root: Node<'_>, offset: usize) -> Option<Node<'_>> {
    if offset > root.end_byte() {
        return None;
    }
    root.descendant_for_byte_range(offset, offset)
}

/// Classifies the token `node` (or its enclosing construct) as a navigable
/// symbol, walking up ancestors when the node itself is not a token of
/// interest.
fn classify_node(mut node: Node<'_>, source: &str) -> Option<Symbol> {
    loop {
        match node.kind() {
            "account" => {
                return Some(Symbol {
                    kind: SymbolKind::Account,
                    name: node_text(node, source).to_string(),
                });
            }
            "commodity_name" => {
                return Some(Symbol {
                    kind: SymbolKind::Commodity,
                    name: node_text(node, source).to_string(),
                });
            }
            "string" => {
                // Only a payee string or an include path are navigable: a
                // transaction's narration or a metadata value is plain text.
                let name = node_text(node, source);
                return match node.parent() {
                    Some(parent) if parent.kind() == "path" => Some(Symbol {
                        kind: SymbolKind::IncludePath,
                        name: strip_quotes(name).to_string(),
                    }),
                    Some(parent)
                        if parent.kind() == "payee" || parent.kind() == "payee_declaration" =>
                    {
                        Some(Symbol {
                            kind: SymbolKind::Payee,
                            name: strip_quotes(name).to_string(),
                        })
                    }
                    _ => None,
                };
            }
            _ => node = node.parent()?,
        }
    }
}

/// The source text covered by `node`.
fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

/// Removes the surrounding double quotes of a `string` token's text.
fn strip_quotes(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(text)
}

/// Extracts every account, payee, commodity and include-path occurrence of
/// `file`, straight from the CST, in document order.
fn occurrences_in(file: &ParsedFile) -> Vec<Occurrence<'_>> {
    let source = file.document().source();
    let root = file.document().root_node();

    let mut occurrences = Vec::new();
    let mut cursor = root.walk();
    for directive in root
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "directive")
    {
        let Some(header) = directive.named_child(0) else {
            continue;
        };

        match header.kind() {
            "open" => {
                if let Some(account) = named_child(header, "account") {
                    push_occurrence(&mut occurrences, source, account, SymbolKind::Account, true);
                }
                // The optional commodity restriction list (`EUR,USD`) nests
                // its `commodity_name` nodes under `commodity_list`.
                if let Some(list) = named_child(header, "commodity_list") {
                    let mut cursor = list.walk();
                    for commodity in list
                        .named_children(&mut cursor)
                        .filter(|n| n.kind() == "commodity_name")
                    {
                        push_occurrence(
                            &mut occurrences,
                            source,
                            commodity,
                            SymbolKind::Commodity,
                            false,
                        );
                    }
                }
            }
            "close" | "balance" => {
                if let Some(account) = named_child(header, "account") {
                    push_occurrence(
                        &mut occurrences,
                        source,
                        account,
                        SymbolKind::Account,
                        false,
                    );
                }
                if header.kind() == "balance"
                    && let Some(commodity) = named_child(header, "commodity_name")
                {
                    push_occurrence(
                        &mut occurrences,
                        source,
                        commodity,
                        SymbolKind::Commodity,
                        false,
                    );
                }
            }
            "commodity" => {
                if let Some(commodity) = named_child(header, "commodity_name") {
                    push_occurrence(
                        &mut occurrences,
                        source,
                        commodity,
                        SymbolKind::Commodity,
                        true,
                    );
                }
            }
            "payee_declaration" => {
                if let Some(string) = named_child(header, "string") {
                    push_occurrence(&mut occurrences, source, string, SymbolKind::Payee, true);
                }
            }
            "transaction" => {
                if let Some(payee) = named_child(header, "payee")
                    && let Some(string) = named_child(payee, "string")
                {
                    push_occurrence(&mut occurrences, source, string, SymbolKind::Payee, false);
                }
                let mut cursor = directive.walk();
                for posting in directive
                    .named_children(&mut cursor)
                    .filter(|n| n.kind() == "posting")
                {
                    if let Some(account) = named_child(posting, "account") {
                        push_occurrence(
                            &mut occurrences,
                            source,
                            account,
                            SymbolKind::Account,
                            false,
                        );
                    }
                    if let Some(amount) = named_child(posting, "amount")
                        && let Some(commodity) = named_child(amount, "commodity_name")
                    {
                        push_occurrence(
                            &mut occurrences,
                            source,
                            commodity,
                            SymbolKind::Commodity,
                            false,
                        );
                    }
                }
            }
            "include" => {
                if let Some(path) = named_child(header, "path")
                    && let Some(string) = named_child(path, "string")
                {
                    push_occurrence(
                        &mut occurrences,
                        source,
                        string,
                        SymbolKind::IncludePath,
                        false,
                    );
                }
            }
            _ => {}
        }
    }

    occurrences
}

fn push_occurrence<'a>(
    occurrences: &mut Vec<Occurrence<'a>>,
    source: &'a str,
    node: Node<'_>,
    kind: SymbolKind,
    is_definition: bool,
) {
    let span = Span::from_node(&node);
    let text = &source[span.byte_range()];
    let name = match kind {
        SymbolKind::Payee | SymbolKind::IncludePath => strip_quotes(text),
        SymbolKind::Account | SymbolKind::Commodity => text,
    };
    occurrences.push(Occurrence {
        kind,
        name,
        span,
        is_definition,
    });
}

fn named_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

/// Collects every occurrence of the scope's files with the location to
/// report for it.
fn scoped_occurrences<'a>(scope: &Scope<'a>) -> Vec<ScopedOccurrence<'a>> {
    let mut collected = Vec::new();
    for entry in &scope.entries {
        let source = entry.file.document().source();
        for occurrence in occurrences_in(entry.file) {
            collected.push(ScopedOccurrence {
                location: entry.uri.as_ref().map(|uri| Location {
                    uri: uri.clone(),
                    range: span_to_range(source, &occurrence.span),
                }),
                path: entry.path.clone(),
                kind: occurrence.kind,
                name: occurrence.name,
                is_definition: occurrence.is_definition,
            });
        }
    }
    collected
}

/// The file an `IncludePath` symbol resolves to, relative to the directory
/// of the file it was written in; `None` for other kinds.
fn include_target(symbol: &Symbol, base_dir: Option<&Path>) -> Option<PathBuf> {
    (symbol.kind == SymbolKind::IncludePath).then(|| resolve_include(base_dir, &symbol.name))
}

/// Whether `occurrence` is the same symbol as `symbol`; for include paths,
/// matching is by *resolved* target file so identical raw paths written from
/// different directories do not cross-match.
fn is_match(occurrence: &ScopedOccurrence, symbol: &Symbol, target: Option<&Path>) -> bool {
    if occurrence.kind != symbol.kind {
        return false;
    }
    match symbol.kind {
        SymbolKind::IncludePath => target.is_some_and(|target| {
            let base = occurrence.path.as_deref().and_then(Path::parent);
            resolve_include(base, occurrence.name).as_path() == target
        }),
        _ => occurrence.name == symbol.name,
    }
}

/// The declaration of `symbol` within the scope, as a location: the `open`
/// of an account, the `payee` directive of a payee, the `commodity`
/// directive of a commodity, or — for an include path — the included file
/// itself. `None` when there is no declaration, which the caller reports as
/// an empty result rather than an error.
fn definition_location(
    scope: &Scope<'_>,
    symbol: &Symbol,
    base_dir: Option<&Path>,
) -> Option<Location> {
    if symbol.kind == SymbolKind::IncludePath {
        let target = resolve_include(base_dir, &symbol.name);
        return scope
            .entries
            .iter()
            .find(|entry| entry.path.as_deref() == Some(target.as_path()))
            .and_then(|entry| entry.uri.clone())
            .map(|uri| Location {
                uri,
                range: Range::default(),
            });
    }

    let target = include_target(symbol, base_dir);
    scoped_occurrences(scope)
        .into_iter()
        .find(|occurrence| {
            occurrence.is_definition && is_match(occurrence, symbol, target.as_deref())
        })
        .and_then(|occurrence| occurrence.location)
}

/// Every occurrence of `symbol` within the scope, sorted by file and
/// position, with the declaration itself included only when
/// `include_declaration` is set.
fn reference_locations(
    scope: &Scope<'_>,
    symbol: &Symbol,
    base_dir: Option<&Path>,
    include_declaration: bool,
) -> Vec<Location> {
    let target = include_target(symbol, base_dir);
    let mut locations: Vec<Location> = scoped_occurrences(scope)
        .into_iter()
        .filter(|occurrence| is_match(occurrence, symbol, target.as_deref()))
        .filter(|occurrence| include_declaration || !occurrence.is_definition)
        .filter_map(|occurrence| occurrence.location)
        .collect();

    // Sorted by URI then position without allocating a key per comparison;
    // the sort makes consecutive duplicates adjacent for `dedup`.
    locations.sort_by(|a, b| {
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then_with(|| a.range.start.line.cmp(&b.range.start.line))
            .then_with(|| a.range.start.character.cmp(&b.range.start.character))
    });
    locations.dedup();
    locations
}

/// Handles `textDocument/definition`: jumps from a use of an account, payee
/// or commodity to its declaration, or from an `include` path to the
/// included file.
pub fn handle_definition(
    config: &Config,
    documents: &Documents,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>, Box<dyn Error + Sync + Send>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?
        .to_string();
    let current_file = pactole_storage_fs::analyze_file(&source);

    let offset = position_to_offset(&source, &position);
    let Some(symbol) = symbol_in_file(&current_file, offset) else {
        return Ok(None);
    };

    // The project is owned by this frame while the scope borrows it, so the
    // scope (and the resolution built from it) cannot outlive the analysis.
    let project = analyze_scope(config, documents, &uri);
    let scope = Scope::build(project.as_ref(), &uri, &current_file);
    let base_dir = base_dir_of(&uri);

    Ok(definition_location(&scope, &symbol, base_dir.as_deref())
        .map(GotoDefinitionResponse::Scalar))
}

/// Handles `textDocument/references`: every occurrence of the symbol at the
/// requested position across the include graph, optionally including its
/// declaration.
pub fn handle_references(
    config: &Config,
    documents: &Documents,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>, Box<dyn Error + Sync + Send>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?
        .to_string();
    let current_file = pactole_storage_fs::analyze_file(&source);

    let offset = position_to_offset(&source, &position);
    let Some(symbol) = symbol_in_file(&current_file, offset) else {
        return Ok(None);
    };

    let project = analyze_scope(config, documents, &uri);
    let scope = Scope::build(project.as_ref(), &uri, &current_file);
    let base_dir = base_dir_of(&uri);

    Ok(Some(reference_locations(
        &scope,
        &symbol,
        base_dir.as_deref(),
        params.context.include_declaration,
    )))
}

/// The directory the current document's raw `include` paths are relative
/// to, when the document has a filesystem path.
fn base_dir_of(uri: &Url) -> Option<PathBuf> {
    uri_to_path(uri).and_then(|path| path.parent().map(Path::to_path_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    use lsp_types::{
        PartialResultParams, Position, ReferenceContext, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use pactole_storage_fs::{InMemorySourceLoader, analyze_file, analyze_file_with_loader};

    fn main_path() -> PathBuf {
        PathBuf::from("/journal/main.pactole")
    }

    fn main_uri() -> Url {
        Url::from_file_path(main_path()).unwrap()
    }

    fn scope<'a>(project: &'a AnalyzedProject, current_file: &'a ParsedFile) -> Scope<'a> {
        Scope::build(Some(project), &main_uri(), current_file)
    }

    fn account_symbol(name: &str) -> Symbol {
        Symbol {
            kind: SymbolKind::Account,
            name: name.to_string(),
        }
    }

    fn payee_symbol(name: &str) -> Symbol {
        Symbol {
            kind: SymbolKind::Payee,
            name: name.to_string(),
        }
    }

    fn commodity_symbol(name: &str) -> Symbol {
        Symbol {
            kind: SymbolKind::Commodity,
            name: name.to_string(),
        }
    }

    fn include_symbol(name: &str) -> Symbol {
        Symbol {
            kind: SymbolKind::IncludePath,
            name: name.to_string(),
        }
    }

    // -----------------------------
    // Symbol detection
    // -----------------------------

    #[test]
    fn finds_the_account_of_a_posting_under_the_cursor() {
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let file = analyze_file(source);
        let offset = source.rfind("Assets:Checking").unwrap() + 3;

        assert_eq!(
            symbol_in_file(&file, offset),
            Some(account_symbol("Assets:Checking"))
        );
    }

    #[test]
    fn finds_a_payee_without_its_quotes() {
        let source = "payee \"Whole Foods\"\n";
        let file = analyze_file(source);
        let offset = source.find("Foods").unwrap() + 2;

        assert_eq!(
            symbol_in_file(&file, offset),
            Some(payee_symbol("Whole Foods"))
        );
    }

    #[test]
    fn finds_the_payee_of_a_transaction() {
        let source = "2026-01-01 * \"Whole Foods\" \"weekly groceries\"\n";
        let file = analyze_file(source);
        let offset = source.find("Whole").unwrap() + 2;

        assert_eq!(
            symbol_in_file(&file, offset),
            Some(payee_symbol("Whole Foods"))
        );
    }

    #[test]
    fn finds_a_commodity_inside_an_open_restriction_list() {
        // `EntrySpans` does not cover the `commodity_list` of an `open`; the
        // CST-based detection does.
        let source = "2026-01-01 open Assets:Bank EUR,USD\n";
        let file = analyze_file(source);
        let offset = source.find("USD").unwrap() + 1;

        assert_eq!(symbol_in_file(&file, offset), Some(commodity_symbol("USD")));
    }

    #[test]
    fn finds_the_commodity_of_a_balance_assertion() {
        let source = "2026-01-01 balance Assets:Bank 100 EUR\n";
        let file = analyze_file(source);
        let offset = source.find("EUR").unwrap();

        assert_eq!(symbol_in_file(&file, offset), Some(commodity_symbol("EUR")));
    }

    #[test]
    fn finds_an_include_path_under_the_cursor() {
        let source = "include \"accounts.pactole\"\n";
        let file = analyze_file(source);
        let offset = source.find("accounts").unwrap() + 2;

        assert_eq!(
            symbol_in_file(&file, offset),
            Some(include_symbol("accounts.pactole"))
        );
    }

    #[test]
    fn finds_a_symbol_when_the_cursor_sits_right_after_a_token() {
        // Editors commonly place the cursor at the very end of a word.
        let source = "2026-01-01 open Assets:Checking\n";
        let file = analyze_file(source);
        let offset = source.find("Assets:Checking").unwrap() + "Assets:Checking".len();

        assert_eq!(
            symbol_in_file(&file, offset),
            Some(account_symbol("Assets:Checking"))
        );
    }

    #[test]
    fn ignores_narrations_and_metadata_values() {
        let source = "2026-01-01 * \"Shop\" \"weekly groceries\"\n  note: \"receipt kept\"\n  Assets:Checking -10 EUR\n";
        let file = analyze_file(source);

        let narration = source.find("groceries").unwrap() + 2;
        assert_eq!(symbol_in_file(&file, narration), None);

        let note = source.find("receipt").unwrap() + 2;
        assert_eq!(symbol_in_file(&file, note), None);
    }

    // -----------------------------
    // Occurrence index and resolution
    // -----------------------------

    #[test]
    fn goto_definition_jumps_from_a_posting_to_its_open() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            main_path(),
            "include \"accounts.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n",
        );
        loader.insert(
            PathBuf::from("/journal/accounts.pactole"),
            "2026-01-01 open Assets:Checking\n2026-01-01 commodity EUR\n",
        );
        let project = analyze_file_with_loader(&main_path(), &loader).expect("project is loadable");
        let current = analyze_file("include \"accounts.pactole\"\n");
        let scope = scope(&project, &current);

        let definition = definition_location(
            &scope,
            &account_symbol("Assets:Checking"),
            Some(Path::new("/journal")),
        )
        .expect("the account is opened somewhere");

        assert_eq!(
            definition.uri,
            Url::from_file_path("/journal/accounts.pactole").unwrap()
        );
        assert_eq!(definition.range.start.line, 0);
    }

    #[test]
    fn goto_definition_returns_none_for_an_undeclared_account() {
        let source = "2026-01-02 * \"Shop\"\n  Assets:Unknown -10 EUR\n";
        let current = analyze_file(source);
        let scope = Scope::build(None, &main_uri(), &current);

        assert_eq!(
            definition_location(&scope, &account_symbol("Assets:Unknown"), None),
            None
        );
    }

    #[test]
    fn goto_definition_on_the_declaration_returns_its_own_location() {
        // The cursor sits on the `open` itself: the declaration is its own
        // definition, as editors expect.
        let source = "2026-01-01 open Assets:Checking\n";
        let file = analyze_file(source);
        let offset = source.find("Assets:Checking").unwrap() + 3;
        let symbol = symbol_in_file(&file, offset).expect("symbol under the cursor");
        let scope = Scope::build(None, &main_uri(), &file);

        let location = definition_location(&scope, &symbol, Some(Path::new("/journal")))
            .expect("the declaration is its own definition");
        assert_eq!(location.uri, main_uri());
        assert_eq!(location.range.start.line, 0);
    }

    #[test]
    fn goto_definition_from_an_included_file_jumps_back_to_the_entry_file() {
        // Reverse direction: the cursor is in an *included* file and resolves
        // to the declaration living in the entry journal.
        let accounts_source = "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let mut loader = InMemorySourceLoader::new();
        loader.insert(main_path(), "2026-01-01 open Assets:Checking\n");
        loader.insert(PathBuf::from("/journal/accounts.pactole"), accounts_source);
        let project = analyze_file_with_loader(&main_path(), &loader).expect("project is loadable");

        let included_uri = Url::from_file_path("/journal/accounts.pactole").unwrap();
        let current = analyze_file(accounts_source);
        let scope = Scope::build(Some(&project), &included_uri, &current);

        let offset = accounts_source.find("Assets:Checking").unwrap() + 3;
        let symbol = symbol_in_file(&current, offset).expect("symbol under the cursor");
        let location = definition_location(&scope, &symbol, Some(Path::new("/journal")))
            .expect("the account is opened in the entry file");
        assert_eq!(location.uri, main_uri());
        assert_eq!(location.range.start.line, 0);
    }

    #[test]
    fn accounts_used_by_close_and_balance_resolve_to_their_open() {
        let source = "2026-01-01 open Assets:Bank\n2026-02-01 close Assets:Bank\n2026-01-15 balance Assets:Bank 100 EUR\n";
        let file = analyze_file(source);
        let scope = Scope::build(None, &main_uri(), &file);

        let close_offset = source.find("close Assets:Bank").unwrap() + "close ".len();
        let close_symbol = symbol_in_file(&file, close_offset).expect("symbol under the cursor");
        let definition = definition_location(&scope, &close_symbol, Some(Path::new("/journal")))
            .expect("the closed account is opened somewhere");
        assert_eq!(definition.uri, main_uri());
        assert_eq!(definition.range.start.line, 0);

        let balance_offset = source.find("balance Assets:Bank").unwrap() + "balance ".len();
        let balance_symbol =
            symbol_in_file(&file, balance_offset).expect("symbol under the cursor");
        let definition = definition_location(&scope, &balance_symbol, Some(Path::new("/journal")))
            .expect("the asserted account is opened somewhere");
        assert_eq!(definition.uri, main_uri());
        assert_eq!(definition.range.start.line, 0);
    }

    #[test]
    fn goto_definition_on_an_include_path_targets_the_included_file() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            main_path(),
            "include \"accounts.pactole\"\ninclude \"missing.pactole\"\n",
        );
        loader.insert(PathBuf::from("/journal/accounts.pactole"), "");
        let project = analyze_file_with_loader(&main_path(), &loader).expect("project is loadable");
        let current = analyze_file("include \"accounts.pactole\"\n");
        let scope = scope(&project, &current);

        let definition = definition_location(
            &scope,
            &include_symbol("accounts.pactole"),
            Some(Path::new("/journal")),
        )
        .expect("the included file is part of the project");
        assert_eq!(
            definition.uri,
            Url::from_file_path("/journal/accounts.pactole").unwrap()
        );

        // An include that resolves to no analyzed file has no definition.
        assert_eq!(
            definition_location(
                &scope,
                &include_symbol("missing.pactole"),
                Some(Path::new("/journal"))
            ),
            None
        );
    }

    #[test]
    fn references_span_the_include_graph_in_sorted_order() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            main_path(),
            "include \"accounts.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food\n",
        );
        loader.insert(
            PathBuf::from("/journal/accounts.pactole"),
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Other\"\n  Assets:Checking -5 EUR\n",
        );
        let project = analyze_file_with_loader(&main_path(), &loader).expect("project is loadable");
        let current = analyze_file("include \"accounts.pactole\"\n");
        let scope = scope(&project, &current);

        let references = reference_locations(
            &scope,
            &account_symbol("Assets:Checking"),
            Some(Path::new("/journal")),
            true,
        );

        let main_uri = main_uri();
        let accounts_uri = Url::from_file_path("/journal/accounts.pactole").unwrap();
        assert_eq!(references.len(), 3);
        // Sorted by file (URI string order: "/journal/accounts..." sorts
        // before "/journal/main...") then position.
        assert_eq!(references[0].uri, accounts_uri);
        assert_eq!(references[0].range.start.line, 0);
        assert_eq!(references[1].uri, accounts_uri);
        assert_eq!(references[1].range.start.line, 2);
        assert_eq!(references[2].uri, main_uri);
        assert_eq!(references[2].range.start.line, 2);
    }

    #[test]
    fn references_can_exclude_the_declaration() {
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let current = analyze_file(source);
        let scope = Scope::build(None, &main_uri(), &current);

        let with_declaration = reference_locations(
            &scope,
            &account_symbol("Assets:Checking"),
            Some(Path::new("/journal")),
            true,
        );
        assert_eq!(with_declaration.len(), 2);

        let without_declaration = reference_locations(
            &scope,
            &account_symbol("Assets:Checking"),
            Some(Path::new("/journal")),
            false,
        );
        assert_eq!(without_declaration.len(), 1);
        assert_eq!(without_declaration[0].range.start.line, 2);
    }

    #[test]
    fn references_of_a_payee_include_its_declaration() {
        let source =
            "payee \"Whole Foods\"\n2026-01-02 * \"Whole Foods\"\n  Assets:Checking -10 EUR\n";
        let current = analyze_file(source);
        let scope = Scope::build(None, &main_uri(), &current);

        let references = reference_locations(
            &scope,
            &payee_symbol("Whole Foods"),
            Some(Path::new("/journal")),
            true,
        );

        assert_eq!(references.len(), 2);
        assert_eq!(references[0].range.start.line, 0);
        assert_eq!(references[1].range.start.line, 1);
    }

    #[test]
    fn include_references_match_by_resolved_path_not_raw_text() {
        // Two identical raw paths written from different directories resolve
        // to different files and must not cross-match.
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            main_path(),
            "include \"shared.pactole\"\ninclude \"other/accounts.pactole\"\n",
        );
        loader.insert(PathBuf::from("/journal/shared.pactole"), "");
        loader.insert(PathBuf::from("/journal/other/accounts.pactole"), "");
        let project = analyze_file_with_loader(&main_path(), &loader).expect("project is loadable");
        let current = analyze_file("include \"shared.pactole\"\n");
        let scope = scope(&project, &current);

        let references = reference_locations(
            &scope,
            &include_symbol("accounts.pactole"),
            Some(Path::new("/journal/other")),
            true,
        );

        // The raw paths differ ("accounts.pactole" vs "other/accounts.pactole")
        // yet resolve to the same file, so the second include of the main
        // journal is a reference; the `shared.pactole` include is not.
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].uri, main_uri());
        assert_eq!(references[0].range.start.line, 1);
    }

    #[test]
    fn the_current_document_is_not_duplicated_when_its_uri_is_not_normalized() {
        // The request URI spells the same file as the project entry but with
        // a `..` segment; the scope must contain one entry for it (echoing
        // the request's own URI), not two entries with diverging URIs — which
        // would duplicate every reference and report goto-definition under
        // the project's spelling instead of the buffer's.
        let mut loader = InMemorySourceLoader::new();
        loader.insert(main_path(), "2026-01-01 open Assets:Checking\n");
        let project = analyze_file_with_loader(&main_path(), &loader).expect("project is loadable");

        let current = analyze_file("2026-01-01 open Assets:Checking\n");
        let unnormalized_uri =
            Url::from_file_path("/journal/../journal/main.pactole").expect("file URL");
        let scope = Scope::build(Some(&project), &unnormalized_uri, &current);

        assert_eq!(scope.entries.len(), 1);
        assert_eq!(scope.entries[0].uri.as_ref(), Some(&unnormalized_uri));
    }

    // -----------------------------
    // Handlers
    // -----------------------------

    fn goto_params(uri: &Url, position: Position) -> GotoDefinitionParams {
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        }
    }

    fn reference_params(
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> ReferenceParams {
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration,
            },
        }
    }

    #[test]
    fn handle_definition_works_on_a_standalone_document() {
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        // The journal file does not exist on disk, so no project is
        // analyzed: the current document alone is the navigation scope.
        let position = Position::new(2, 4);
        let result = handle_definition(&Config::default(), &documents, goto_params(&uri, position))
            .unwrap()
            .expect("the account has a declaration");

        let GotoDefinitionResponse::Scalar(location) = result else {
            panic!("expected a single location");
        };
        assert_eq!(location.uri, uri);
        assert_eq!(location.range.start.line, 0);
    }

    #[test]
    fn handle_definition_returns_no_result_without_a_symbol() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "\n\n2026-01-01 open Assets:Checking\n".to_string(),
        );

        let result = handle_definition(
            &Config::default(),
            &documents,
            goto_params(&uri, Position::new(0, 0)),
        )
        .unwrap();

        assert_eq!(result, None);
    }

    #[test]
    fn handle_references_lists_every_occurrence_of_the_account() {
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let references = handle_references(
            &Config::default(),
            &documents,
            reference_params(&uri, Position::new(2, 4), true),
        )
        .unwrap()
        .expect("references of a declared symbol");

        assert_eq!(references.len(), 2);
        assert_eq!(references[0].range.start.line, 0);
        assert_eq!(references[1].range.start.line, 2);
    }

    #[test]
    fn handle_references_can_exclude_the_declaration_at_handler_level() {
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let references = handle_references(
            &Config::default(),
            &documents,
            reference_params(&uri, Position::new(2, 4), false),
        )
        .unwrap()
        .expect("references of a declared symbol");

        assert_eq!(references.len(), 1);
        assert_eq!(references[0].range.start.line, 2);
    }

    #[test]
    fn handle_definition_works_on_a_document_without_a_file_path() {
        // An `untitled:` buffer has no directory to resolve includes from
        // and no on-disk entry point; the document itself is the scope.
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let uri = Url::parse("untitled:Untitled-1").expect("valid URI");
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let result = handle_definition(
            &Config::default(),
            &documents,
            goto_params(&uri, Position::new(2, 4)),
        )
        .unwrap()
        .expect("the account has a declaration");

        let GotoDefinitionResponse::Scalar(location) = result else {
            panic!("expected a single location");
        };
        assert_eq!(location.uri, uri);
        assert_eq!(location.range.start.line, 0);
    }

    #[test]
    fn handle_definition_uses_unsaved_buffers_of_included_files() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-navigation-test-{}-unsaved",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.pactole");
        let accounts = dir.join("accounts.pactole");
        std::fs::write(
            &main,
            "include \"accounts.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:New -10 EUR\n",
        )
        .unwrap();
        // On disk, the included file still opens the old account name.
        std::fs::write(&accounts, "2026-01-01 open Assets:Old\n").unwrap();

        let main_uri = Url::from_file_path(&main).expect("file URL");
        let accounts_uri = Url::from_file_path(&accounts).expect("file URL");
        let mut documents = Documents::new();
        documents.open(main_uri.clone(), std::fs::read_to_string(&main).unwrap());
        // The editor has an unsaved edit renaming the account; without the
        // buffer taking precedence over disk, `Assets:New` would resolve to
        // no declaration at all.
        documents.open(
            accounts_uri.clone(),
            "2026-01-01 open Assets:New\n".to_string(),
        );

        let mut config = Config::default();
        config.journal_file = Some(main);

        let result = handle_definition(
            &config,
            &documents,
            goto_params(&main_uri, Position::new(2, 4)),
        )
        .unwrap()
        .expect("the unsaved buffer declares the account");

        let GotoDefinitionResponse::Scalar(location) = result else {
            panic!("expected a single location");
        };
        assert_eq!(location.uri, accounts_uri);
        assert_eq!(location.range.start.line, 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
