//! Recovers what a file's analysis could neither confirm nor rule out.
//!
//! When a directive cannot be turned into a domain entry, the content it
//! carried is lost. The names it mentioned may have been *declared* there (an
//! account opened, a commodity or a payee declared) or *used* there (a
//! posting, a balance assertion), so the business checks that depend on
//! those facts are uncertain and must not be reported.
//!
//! Tree-sitter's error recovery is directive-local: a broken directive does
//! not swallow the directives that follow it, and the concrete syntax tree of
//! a broken directive keeps the `account`, `commodity_name` and `string`
//! nodes the parser managed to recognize. The names are therefore read from
//! the tree rather than guessed from the raw text, which keeps the
//! uncertainty as narrow as the source allows.

use pactole_core::{AccountName, CommodityName, MissingData};
use tree_sitter::Node;

/// Records the names a directive that could not be lowered mentions.
///
/// The directive was recognized by the parser, so the kind of directive is
/// known and the names are recorded on the axis that matches what the
/// directive could have done: an `open` could have declared an account, a
/// transaction could have used one.
pub(crate) fn record_lost_directive(missing: &mut MissingData, directive: Node<'_>, source: &str) {
    let Some(header) = directive.named_child(0) else {
        record_unknown(missing, directive, source);
        return;
    };

    match header.kind() {
        "open" => {
            for account in accounts_in(directive, source) {
                missing.accounts.insert(account);
            }
            // The restricted list of an `open` directive references the
            // commodities it names.
            for commodity in commodities_in(directive, source) {
                missing.used_commodities.insert(commodity);
            }
        }
        "commodity" => {
            for commodity in commodities_in(directive, source) {
                missing.commodities.insert(commodity);
            }
        }
        "payee_declaration" => {
            for payee in payees_in(directive, source) {
                missing.payees.insert(payee);
            }
        }
        "transaction" => {
            for account in accounts_in(directive, source) {
                missing.used_accounts.insert(account);
            }
            for commodity in commodities_in(directive, source) {
                missing.used_commodities.insert(commodity);
            }
            for payee in payees_in(directive, source) {
                missing.used_payees.insert(payee);
            }
        }
        "balance" => {
            for account in accounts_in(directive, source) {
                missing.used_accounts.insert(account);
            }
            for commodity in commodities_in(directive, source) {
                missing.used_commodities.insert(commodity);
            }
        }
        // A `close` directive neither declares nor uses anything, and an
        // `include` directive names no account at all.
        _ => {}
    }
}

/// Records the names a chunk of source whose meaning is unknown mentions.
///
/// Every name is recorded on both axes: the chunk may have declared it, or
/// used it.
pub(crate) fn record_unknown(missing: &mut MissingData, node: Node<'_>, source: &str) {
    for account in accounts_in(node, source) {
        missing.accounts.insert(account.clone());
        missing.used_accounts.insert(account);
    }
    for commodity in commodities_in(node, source) {
        missing.commodities.insert(commodity.clone());
        missing.used_commodities.insert(commodity);
    }
    for payee in payees_in(node, source) {
        missing.payees.insert(payee.clone());
        missing.used_payees.insert(payee);
    }
}

/// Whether the subtree contains a syntax error or a missing node, i.e. whether
/// the parser could not make sense of the whole directive.
///
/// A directive that lowers despite a syntax error is not reliable: the error
/// may have swallowed part of a name (an unterminated string, a truncated
/// account, ...).
pub(crate) fn is_degraded(node: Node<'_>) -> bool {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.is_error() || current.is_missing() {
            return true;
        }
        let mut cursor = current.walk();
        let children: Vec<Node<'_>> = current.children(&mut cursor).collect();
        stack.extend(children);
    }
    false
}

/// The accounts named in the subtree of `node`.
fn accounts_in(node: Node<'_>, source: &str) -> Vec<AccountName> {
    texts_in(node, source, "account")
        .into_iter()
        .filter_map(|text| AccountName::new(text).ok())
        .collect()
}

/// The commodities named in the subtree of `node`.
fn commodities_in(node: Node<'_>, source: &str) -> Vec<CommodityName> {
    texts_in(node, source, "commodity_name")
        .into_iter()
        .filter_map(|text| CommodityName::new(text).ok())
        .collect()
}

/// The payees named in the subtree of `node`.
///
/// Only the strings that are payees are collected: a transaction's payee and
/// a payee declaration. Narration, path and metadata strings are not payees.
fn payees_in(node: Node<'_>, source: &str) -> Vec<String> {
    let mut payees = Vec::new();
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        if current.kind() == "string" {
            let is_payee = current
                .parent()
                .is_some_and(|parent| matches!(parent.kind(), "payee" | "payee_declaration"));
            if is_payee && let Ok(text) = current.utf8_text(source.as_bytes()) {
                let name = text.trim_matches('"');
                if !name.is_empty() {
                    payees.push(name.to_string());
                }
            }
            continue;
        }

        let mut cursor = current.walk();
        let children: Vec<Node<'_>> = current.named_children(&mut cursor).collect();
        stack.extend(children);
    }

    payees
}

/// The texts of the nodes of kind `kind` in the subtree of `node`.
fn texts_in(node: Node<'_>, source: &str, kind: &str) -> Vec<String> {
    let mut texts = Vec::new();
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        if current.kind() == kind {
            if let Ok(text) = current.utf8_text(source.as_bytes()) {
                texts.push(text.to_string());
            }
            continue;
        }

        let mut cursor = current.walk();
        let children: Vec<Node<'_>> = current.named_children(&mut cursor).collect();
        stack.extend(children);
    }

    texts
}
