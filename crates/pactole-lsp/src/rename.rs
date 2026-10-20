//! Rename support for `.pactole` files.
//!
//! Implements `textDocument/prepareRename` and `textDocument/rename` for the
//! two renamable symbol kinds, accounts and payees. Both requests resolve
//! against the exact same occurrence index and scope as navigation (see
//! [`crate::navigation`]): the journal's whole include graph, with unsaved
//! editor buffers taking precedence over what is on disk.
//!
//! An account is renamed by replacing each token as written in the source (an
//! `open`, a `close` or `balance`, a posting account). A payee is renamed by
//! replacing only the *content* of its `string` token, so the surrounding
//! quotes are preserved (a `string` token is single-line and unescaped per
//! the grammar, which makes the inner range a ±1-character adjustment of the
//! token range). The adjustment is only applied to a token that really
//! carries both quotes (see `payee_content_range`): a token left
//! unterminated by error recovery is refused instead of mis-edited.
//!
//! Following the roadmap, a position with no symbol — or a symbol that is not
//! renameable (a commodity, an `include` path, a date, a narration, a tag) —
//! yields an empty result, never an error. Invalid rename *input* is a
//! different matter: an invalid new name, or one that would create a
//! duplicate declaration (`open` or `payee`) across the scope, is reported to
//! the editor as a [`RenameError`], which the server maps to an
//! `InvalidParams` response so nothing is applied.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use lsp_types::{
    PrepareRenameResponse, Range, RenameParams, TextDocumentPositionParams, TextEdit, Url,
    WorkspaceEdit,
};

use crate::config::Config;
use crate::conversion::{position_to_offset, span_to_range};
use crate::documents::{Documents, analyze_scope};
use crate::navigation::{
    Scope, ScopedOccurrence, SymbolKind, is_match, scoped_occurrences, symbol_in_file,
    symbol_with_span,
};

/// An invalid rename request (an unusable new name, or one conflicting with
/// an existing declaration), reported to the client as an `InvalidParams`
/// error rather than a generic internal error.
#[derive(Debug)]
pub struct RenameError(String);

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for RenameError {}

/// Handles `textDocument/rename`: every occurrence of the account or payee
/// under the cursor across the navigation scope, as a single
/// [`WorkspaceEdit`]. Non-renamable symbols, positions without a symbol, and
/// a rename to the same name yield `Ok(None)`; an invalid or conflicting new
/// name is a [`RenameError`].
pub fn handle_rename(
    config: &Config,
    documents: &Documents,
    params: RenameParams,
) -> Result<Option<WorkspaceEdit>, Box<dyn Error + Sync + Send>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let new_name = params.new_name;

    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?
        .to_string();
    let current_file = pactole_storage_fs::analyze_file(&source);

    let offset = position_to_offset(&source, &position);
    let Some(symbol) = symbol_in_file(&current_file, offset) else {
        return Ok(None);
    };
    if !matches!(symbol.kind, SymbolKind::Account | SymbolKind::Payee) {
        return Ok(None);
    }
    if new_name == symbol.name {
        return Ok(None);
    }
    validate_new_name(symbol.kind, &new_name)?;

    // The project is owned by this frame while the scope borrows it, so the
    // occurrences (and the edits built from them) cannot outlive the analysis.
    let project = analyze_scope(config, documents, &uri);
    // A configured journal that fails to analyze would silently narrow the
    // scope to the current file only, applying a partial rename across the
    // journal: refuse instead of mutating a subset of the files.
    if config.journal_file.is_some() && project.is_none() {
        return Err(RenameError(
            "cannot analyze the configured journal file; rename would be incomplete across the journal, fix the journal first"
                .to_string(),
        )
        .into());
    }
    let scope = Scope::build(project.as_ref(), &uri, &current_file);

    let occurrences = scoped_occurrences(&scope);
    check_conflict(&occurrences, symbol.kind, &symbol.name, &new_name)?;

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for occurrence in &occurrences {
        if !is_match(occurrence, &symbol, None) {
            continue;
        }
        let Some(location) = &occurrence.location else {
            // A file with no URI cannot be edited from the client side; in
            // practice only an untitled buffer outside the request itself
            // could get here. Never log the occurrence's name: it is
            // document content (see the README's logging policy).
            log::warn!(
                "skipping a {:?} rename occurrence without a URI in {:?}",
                occurrence.kind,
                occurrence.path
            );
            continue;
        };
        let range = if occurrence.kind == SymbolKind::Payee {
            // Keep the surrounding quotes: replace only the string's
            // content. A degenerate token (error recovery left the string
            // unterminated) cannot be edited safely: refuse the whole
            // rename rather than corrupting that file.
            match payee_content_range(&location.range, occurrence.name) {
                Some(range) => range,
                None => {
                    return Err(RenameError(format!(
                        "cannot rename `{}`: an occurrence is an unterminated string (missing closing quote), fix it first",
                        occurrence.name
                    ))
                    .into());
                }
            }
        } else {
            location.range
        };
        changes
            .entry(location.uri.clone())
            .or_default()
            .push(TextEdit {
                range,
                new_text: new_name.clone(),
            });
    }

    if changes.is_empty() {
        return Ok(None);
    }
    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}

/// Handles `textDocument/prepareRename`: the renameable range (and
/// placeholder) of the account or payee under the cursor, `Ok(None)`
/// everywhere else so the editor reports the position as not renameable. The
/// payee range excludes the surrounding quotes, and so does the placeholder:
/// the editor prefills the rename input with it.
pub fn handle_prepare_rename(
    documents: &Documents,
    params: TextDocumentPositionParams,
) -> Result<Option<PrepareRenameResponse>, Box<dyn Error + Sync + Send>> {
    let uri = params.text_document.uri;
    let position = params.position;

    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?
        .to_string();
    let current_file = pactole_storage_fs::analyze_file(&source);

    let offset = position_to_offset(&source, &position);
    let Some((symbol, span)) = symbol_with_span(&current_file, offset) else {
        return Ok(None);
    };

    let range = span_to_range(&source, &span);
    match symbol.kind {
        SymbolKind::Account => Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range,
            placeholder: symbol.name,
        })),
        SymbolKind::Payee => {
            // The inner range is only offered on a well-formed `"…"` token;
            // a token left unterminated by error recovery has no safe inner
            // range, so the position is reported as not renameable.
            let Some(content) = payee_content_range(&range, &symbol.name) else {
                return Ok(None);
            };
            Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: content,
                placeholder: symbol.name,
            }))
        }
        _ => Ok(None),
    }
}

/// Returns the range of a payee `string` token's *content* (the token range
/// with the surrounding quotes removed), or `None` for a degenerate token.
///
/// A well-formed token is `"` + content + `"` with single UTF-16-unit quotes
/// and no escape, so its length in UTF-16 units is exactly the name's length
/// plus two. Error recovery can leave a string unterminated (a `MISSING`
/// closing-quote node inside the `string` node), in which case the name still
/// carries the leading quote and this length check fails; a zero-width
/// `MISSING` token fails it too. Adjusting such a range would drop (or
/// duplicate) a real character of the source, so callers refuse the rename
/// instead of applying a corrupted edit.
fn payee_content_range(range: &Range, name: &str) -> Option<Range> {
    let token_units = range.end.character.checked_sub(range.start.character)?;
    let quoted_units = u32::try_from(name.encode_utf16().count()).ok()? + 2;
    if token_units != quoted_units {
        return None;
    }
    let mut content = *range;
    content.start.character += 1;
    content.end.character -= 1;
    Some(content)
}

/// Validates the requested new name for `kind` against the grammar only
/// (scope conflicts are checked separately by [`check_conflict`]). Accounts
/// reuse the core parser so the error message matches the rest of the
/// toolchain; payees must be non-empty and contain no double quote or
/// newline (a `string` token is `seq('"', /[^"\r\n]*/, '"')`).
fn validate_new_name(kind: SymbolKind, new_name: &str) -> Result<(), RenameError> {
    match kind {
        SymbolKind::Account => {
            pactole_core::AccountName::new(new_name).map_err(|err| RenameError(err.to_string()))?;
        }
        SymbolKind::Payee if new_name.is_empty() || new_name.contains(['"', '\n', '\r']) => {
            return Err(RenameError(format!(
                "invalid payee name `{new_name}`: it must be non-empty and contain no double quote or newline"
            )));
        }
        // A valid payee name, and the kinds that never reach this
        // validation (only accounts and payees are renameable): nothing
        // more to check.
        _ => {}
    }
    Ok(())
}

/// Refuses a rename whose target name is already *declared* in the scope
/// while the source name is declared too: applying it would create a
/// duplicate `open` or `payee` directive. Merging the other way (undeclared
/// uses moved onto an already-declared name) stays allowed.
fn check_conflict(
    occurrences: &[ScopedOccurrence<'_>],
    kind: SymbolKind,
    old_name: &str,
    new_name: &str,
) -> Result<(), RenameError> {
    let is_declaration_of = |occurrence: &ScopedOccurrence<'_>, name: &str| {
        occurrence.is_definition && occurrence.kind == kind && occurrence.name == name
    };
    let source_declared = occurrences
        .iter()
        .any(|occurrence| is_declaration_of(occurrence, old_name));
    let target_declared = occurrences
        .iter()
        .any(|occurrence| is_declaration_of(occurrence, new_name));
    if source_declared && target_declared {
        return Err(RenameError(format!(
            "cannot rename `{old_name}` to `{new_name}`: `{new_name}` is already declared in the journal; remove its declaration first to merge the two"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use lsp_types::{Position, TextDocumentIdentifier, WorkDoneProgressParams};

    fn main_path() -> std::path::PathBuf {
        std::path::PathBuf::from("/journal/main.pactole")
    }

    fn main_uri() -> Url {
        Url::from_file_path(main_path()).unwrap()
    }

    fn prepare_params(uri: &Url, position: Position) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        }
    }

    fn rename_params(uri: &Url, position: Position, new_name: &str) -> RenameParams {
        RenameParams {
            text_document_position: prepare_params(uri, position),
            new_name: new_name.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        }
    }

    fn edits_in<'a>(edit: &'a WorkspaceEdit, uri: &Url) -> &'a [TextEdit] {
        edit.changes
            .as_ref()
            .and_then(|changes| changes.get(uri))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    // -----------------------------
    // prepareRename
    // -----------------------------

    #[test]
    fn prepare_rename_reports_the_account_token() {
        let source = "2026-01-01 open Assets:Checking\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let response =
            handle_prepare_rename(&documents, prepare_params(&uri, Position::new(0, 18)))
                .unwrap()
                .expect("accounts are renameable");

        let PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } = response else {
            panic!("expected a range with a placeholder");
        };
        assert_eq!(placeholder, "Assets:Checking");
        assert_eq!(range.start, Position::new(0, 16));
        assert_eq!(range.end, Position::new(0, 31));
    }

    #[test]
    fn prepare_rename_reports_the_payee_text_inside_the_quotes() {
        let source = "2026-01-02 * \"Whole Foods\" \"groceries\"\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let response =
            handle_prepare_rename(&documents, prepare_params(&uri, Position::new(0, 15)))
                .unwrap()
                .expect("payees are renameable");

        let PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } = response else {
            panic!("expected a range with a placeholder");
        };
        // The range and the placeholder exclude the surrounding quotes: the
        // editor prefills the rename input with the placeholder.
        assert_eq!(placeholder, "Whole Foods");
        assert_eq!(range.start, Position::new(0, 14));
        assert_eq!(range.end, Position::new(0, 25));
    }

    #[test]
    fn prepare_rename_is_empty_for_other_symbols() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "2026-01-01 commodity EUR\n2026-01-02 * \"Shop\" \"groceries\"\n".to_string(),
        );

        // On the commodity name...
        let commodity =
            handle_prepare_rename(&documents, prepare_params(&uri, Position::new(0, 22))).unwrap();
        assert!(commodity.is_none(), "commodities are not renameable");
        // ... and inside the narration string.
        let narration =
            handle_prepare_rename(&documents, prepare_params(&uri, Position::new(1, 22))).unwrap();
        assert!(narration.is_none(), "narrations are not renameable");
    }

    #[test]
    fn prepare_rename_is_empty_for_an_unterminated_payee_string() {
        // Error recovery keeps the `string` node but inserts a `MISSING`
        // closing quote (see the `pactole-syntax` tests): the token has no
        // safe inner range, so the position is not renameable.
        let source = "payee \"abc\n2026-01-02 * \"abc\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let response =
            handle_prepare_rename(&documents, prepare_params(&uri, Position::new(0, 7))).unwrap();
        assert!(
            response.is_none(),
            "an unterminated string is not renameable"
        );
    }

    // -----------------------------
    // No-op and empty results
    // -----------------------------

    #[test]
    fn handle_rename_is_empty_without_a_symbol() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "2026-01-01 open Assets:Checking\n".to_string());

        // On the date, not on the account name.
        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(0, 2), "Assets:Current"),
        )
        .unwrap();
        assert!(edit.is_none());
    }

    #[test]
    fn handle_rename_is_a_noop_for_the_same_name() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "2026-01-01 open Assets:Checking\n".to_string());

        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(0, 18), "Assets:Checking"),
        )
        .unwrap();
        assert!(edit.is_none());
    }

    #[test]
    fn handle_rename_is_empty_for_non_renamable_symbols() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "2026-01-01 commodity EUR\n".to_string());

        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(0, 22), "USD"),
        )
        .unwrap();
        assert!(edit.is_none(), "commodities are not renameable");
    }

    // -----------------------------
    // Renaming
    // -----------------------------

    #[test]
    fn renames_an_account_across_the_current_file() {
        let source =
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(2, 4), "Assets:Current"),
        )
        .unwrap()
        .expect("the account is renameable");

        assert_eq!(
            edits_in(&edit, &uri),
            &[
                TextEdit {
                    range: Range::new(Position::new(0, 16), Position::new(0, 31)),
                    new_text: "Assets:Current".to_string(),
                },
                TextEdit {
                    range: Range::new(Position::new(2, 2), Position::new(2, 17)),
                    new_text: "Assets:Current".to_string(),
                },
            ]
        );
    }

    #[test]
    fn renames_an_account_in_close_and_balance_directives() {
        let source = "2026-01-01 open Assets:Old\n2026-02-01 balance Assets:Old 100 EUR\n2026-03-01 close Assets:Old\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(1, 20), "Assets:New"),
        )
        .unwrap()
        .expect("the account is renameable");

        assert_eq!(
            edits_in(&edit, &uri),
            &[
                TextEdit {
                    range: Range::new(Position::new(0, 16), Position::new(0, 26)),
                    new_text: "Assets:New".to_string(),
                },
                TextEdit {
                    range: Range::new(Position::new(1, 19), Position::new(1, 29)),
                    new_text: "Assets:New".to_string(),
                },
                TextEdit {
                    range: Range::new(Position::new(2, 17), Position::new(2, 27)),
                    new_text: "Assets:New".to_string(),
                },
            ]
        );
    }

    #[test]
    fn renames_an_account_across_the_include_graph() {
        let main_source =
            "include \"accounts.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n";
        let accounts_source = "2026-01-01 open Assets:Checking\n";

        let uri = main_uri();
        let accounts_uri = Url::from_file_path("/journal/accounts.pactole").expect("file URL");
        let mut documents = Documents::new();
        documents.open(uri.clone(), main_source.to_string());
        documents.open(accounts_uri.clone(), accounts_source.to_string());

        let mut config = Config::default();
        config.journal_file = Some(main_path());

        let edit = handle_rename(
            &config,
            &documents,
            rename_params(&uri, Position::new(2, 4), "Assets:Current"),
        )
        .unwrap()
        .expect("the account is renameable");

        assert_eq!(
            edits_in(&edit, &uri),
            &[TextEdit {
                range: Range::new(Position::new(2, 2), Position::new(2, 17)),
                new_text: "Assets:Current".to_string(),
            }]
        );
        assert_eq!(
            edits_in(&edit, &accounts_uri),
            &[TextEdit {
                range: Range::new(Position::new(0, 16), Position::new(0, 31)),
                new_text: "Assets:Current".to_string(),
            }]
        );
    }

    #[test]
    fn renames_a_payee_keeping_the_quotes_and_handling_utf8() {
        let source = "payee \"café\"\n2026-01-02 * \"café\"\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(1, 15), "café organic"),
        )
        .unwrap()
        .expect("the payee is renameable");

        // Each edit replaces only the string's content, so the surrounding
        // quotes are preserved; `é` is a single UTF-16 unit, keeping the
        // character offsets in sync with the byte-level spans.
        assert_eq!(
            edits_in(&edit, &uri),
            &[
                TextEdit {
                    range: Range::new(Position::new(0, 7), Position::new(0, 11)),
                    new_text: "café organic".to_string(),
                },
                TextEdit {
                    range: Range::new(Position::new(1, 14), Position::new(1, 18)),
                    new_text: "café organic".to_string(),
                },
            ]
        );
    }

    // -----------------------------
    // Validation
    // -----------------------------

    #[test]
    fn rename_refuses_an_unterminated_payee_string() {
        // Same degenerate token as
        // `prepare_rename_is_empty_for_an_unterminated_payee_string`: the
        // inner range would drop the last character of the token, so the
        // rename is refused instead of corrupting the file.
        let source = "payee \"abc\n2026-01-02 * \"abc\n  Assets:Checking -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let error = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(1, 15), "abd"),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("unterminated string"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn rename_refuses_when_the_journal_cannot_be_analyzed() {
        // A configured journal that fails to load would silently narrow the
        // rename to the current file only: the request is refused instead.
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "2026-01-01 open Assets:Checking\n".to_string());

        let mut config = Config::default();
        config.journal_file = Some(std::path::PathBuf::from("/missing/main.pactole"));

        let error = handle_rename(
            &config,
            &documents,
            rename_params(&uri, Position::new(0, 18), "Assets:Current"),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("cannot analyze"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn rename_rejects_an_invalid_account_name() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "2026-01-01 open Assets:Checking\n".to_string());

        let error = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(0, 18), "assets:Current"),
        )
        .unwrap_err();

        // The core parser's message, reused verbatim.
        assert!(
            error.to_string().contains("invalid account name"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn rename_rejects_an_invalid_payee_name() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "payee \"Whole Foods\"\n".to_string());

        for new_name in ["bad\"name", "bad\nname", ""] {
            let error = handle_rename(
                &Config::default(),
                &documents,
                rename_params(&uri, Position::new(0, 6), new_name),
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("invalid payee name"),
                "unexpected message for {new_name:?}: {error}"
            );
        }
    }

    #[test]
    fn rename_refuses_a_conflicting_account_declaration() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "2026-01-01 open Assets:Old\n2026-01-01 open Assets:New\n".to_string(),
        );

        let error = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(0, 16), "Assets:New"),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("already declared"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn rename_allows_merging_uses_into_an_existing_account() {
        let source = "2026-01-01 open Assets:New\n2026-01-02 * \"Shop\"\n  Assets:Old -10 EUR\n";
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        // The source account is undeclared, so moving its uses onto the
        // declared target creates no duplicate `open`.
        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(2, 4), "Assets:New"),
        )
        .unwrap()
        .expect("the merge is allowed");

        assert_eq!(
            edits_in(&edit, &uri),
            &[TextEdit {
                range: Range::new(Position::new(2, 2), Position::new(2, 12)),
                new_text: "Assets:New".to_string(),
            }]
        );
    }

    #[test]
    fn rename_refuses_a_conflicting_payee_declaration() {
        let uri = main_uri();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "payee \"A\"\npayee \"B\"\n2026-01-02 * \"B\"\n  Assets:Checking -10 EUR\n".to_string(),
        );

        let error = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(1, 6), "A"),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("already declared"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn rename_refuses_a_conflicting_declaration_across_files() {
        let main_source =
            "include \"accounts.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:Old -10 EUR\n";
        let accounts_source = "2026-01-01 open Assets:Old\n2026-01-01 open Assets:New\n";

        let uri = main_uri();
        let accounts_uri = Url::from_file_path("/journal/accounts.pactole").expect("file URL");
        let mut documents = Documents::new();
        documents.open(uri.clone(), main_source.to_string());
        documents.open(accounts_uri.clone(), accounts_source.to_string());

        let mut config = Config::default();
        config.journal_file = Some(main_path());

        // Both declarations are in the scope (one per file), so merging
        // would still create a duplicate `open`.
        let error = handle_rename(
            &config,
            &documents,
            rename_params(&uri, Position::new(2, 4), "Assets:New"),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("already declared"),
            "unexpected message: {error}"
        );
    }

    // -----------------------------
    // Editor buffers and untitled documents
    // -----------------------------

    #[test]
    fn handle_rename_uses_unsaved_buffers_of_included_files() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-rename-test-{}-unsaved",
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
        // The editor has an unsaved edit opening the new account name; the
        // rename must cover that buffer, not its outdated on-disk content.
        documents.open(
            accounts_uri.clone(),
            "2026-01-01 open Assets:New\n".to_string(),
        );

        let mut config = Config::default();
        config.journal_file = Some(main);

        let edit = handle_rename(
            &config,
            &documents,
            rename_params(&main_uri, Position::new(2, 4), "Assets:Checking"),
        )
        .unwrap()
        .expect("the account is renameable");

        assert_eq!(
            edits_in(&edit, &main_uri),
            &[TextEdit {
                range: Range::new(Position::new(2, 2), Position::new(2, 12)),
                new_text: "Assets:Checking".to_string(),
            }]
        );
        assert_eq!(
            edits_in(&edit, &accounts_uri),
            &[TextEdit {
                range: Range::new(Position::new(0, 16), Position::new(0, 26)),
                new_text: "Assets:Checking".to_string(),
            }]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handle_rename_edits_untitled_documents() {
        let uri = Url::parse("untitled:Untitled-1").unwrap();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "2026-01-01 open Assets:Checking\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n"
                .to_string(),
        );

        let edit = handle_rename(
            &Config::default(),
            &documents,
            rename_params(&uri, Position::new(0, 18), "Assets:Current"),
        )
        .unwrap()
        .expect("the account is renameable");

        assert_eq!(
            edits_in(&edit, &uri),
            &[
                TextEdit {
                    range: Range::new(Position::new(0, 16), Position::new(0, 31)),
                    new_text: "Assets:Current".to_string(),
                },
                TextEdit {
                    range: Range::new(Position::new(2, 2), Position::new(2, 17)),
                    new_text: "Assets:Current".to_string(),
                },
            ]
        );
    }
}
