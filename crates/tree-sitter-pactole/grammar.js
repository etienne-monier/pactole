/**
 * @file A parser for pactole file
 * @author Etienne Monier
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "pactole",

  // Only comments are handled explicitly at documented positions (see
  // `_trailing_comment` and `_comment_line` below); no implicit
  // whitespace-skipping is introduced by `extras`, since every required
  // separator is already produced by the explicit `_ws` token below.
  extras: () => [],

  // The grammar cannot locally tell, right after a posting's account line,
  // whether the following indented line starts this posting's own
  // metadata (`property`) or is the next posting in the directive: both
  // start with the same flexible `_ws` token. Let the GLR parser explore
  // both branches; they always resolve once the `key ":"` vs `account`
  // shape is seen.
  conflicts: ($) => [
    [$.posting],
    // A standalone comment line right after a transaction's own metadata
    // block can either still be part of that metadata block or already
    // be the first line of the posting block below (both start with the
    // same flexible `_comment_line`); let the GLR parser explore both,
    // they always resolve once a `property` or `posting` is next seen.
    [$._property_or_comment, $._posting_or_comment],
  ],

  rules: {
    source_file: ($) => repeat(choice($.directive, $.comment, $._newline)),

    // Directive-level metadata (`property`) must come before any posting:
    // this keeps the grammar unambiguous while allowing a flexible amount
    // of leading whitespace (see `_ws`), since a `key: value` line right
    // after a posting is otherwise indistinguishable from one belonging to
    // the directive itself. Only `transaction` may be followed by
    // postings: `open`/`close`/`commodity`/`payee`/`balance`/`include`
    // never have postings of their own.
    directive: ($) =>
      choice(
        seq($.open, $._newline, repeat($._property_or_comment)),
        seq($.close, $._newline, repeat($._property_or_comment)),
        seq($.commodity, $._newline, repeat($._property_or_comment)),
        seq($.payee_declaration, $._newline, repeat($._property_or_comment)),
        seq($.balance, $._newline, repeat($._property_or_comment)),
        seq($.include, $._newline, repeat($._property_or_comment)),
        seq(
          $.transaction,
          $._newline,
          repeat($._property_or_comment),
          repeat($._posting_or_comment),
        ),
      ),

    // A comment following the last meaningful token of a line, e.g.
    // `2026-01-01 commodity EUR ; a note`. Only allowed right before the
    // line's `_newline`, at the documented positions (directive/header
    // end, posting end, property end): never silently anywhere, unlike
    // `tree-sitter-beancount`'s catch-all `extras`. Fielded as `comment`
    // so it can be told apart, on the Rust side, from a standalone
    // `_comment_line` node that happens to be a sibling in the same
    // repeated block (see `_property_or_comment`/`_posting_or_comment`).
    _trailing_comment: ($) => seq($._ws, field("comment", $.comment)),

    // A standalone comment line at posting/property indentation, e.g. a
    // comment between two postings or between two properties. Kept as a
    // plain `comment` node in the tree (no wrapper node), interleaved with
    // `property`/`posting` in source order.
    _comment_line: ($) => seq($._ws, $.comment, $._newline),

    _property_or_comment: ($) => choice($.property, $._comment_line),
    _posting_or_comment: ($) => choice($.posting, $._comment_line),

    open: ($) =>
      seq(
        $.date,
        $._ws,
        "open",
        $._ws,
        $.account,
        optional(seq($._ws, $.commodity_list)),
        optional($._trailing_comment),
      ),
    // Beancount-style optional list of commodities an account is
    // meant to be restricted to, e.g. `EUR` or `EUR,USD`: comma-separated,
    // no spaces around the comma. This is currently only parsed and
    // stored; it is not enforced (postings using other commodities are
    // not rejected).
    commodity_list: ($) =>
      seq($.commodity_name, repeat(seq(",", $.commodity_name))),
    close: ($) =>
      seq($.date, $._ws, "close", $._ws, $.account, optional($._trailing_comment)),
    commodity: ($) =>
      seq(
        $.date,
        $._ws,
        "commodity",
        $._ws,
        $.commodity_name,
        optional($._trailing_comment),
      ),
    // Declares a payee as "known". Unlike `open`/`commodity`, this takes
    // no date: a payee has no temporal life cycle (it is never "opened"
    // or superseded), so only a presence check makes sense — a
    // transaction's payee must match one of these declarations
    // *somewhere* in the file, regardless of relative order (see
    // `ValidationError::PayeeNotDeclared` on the Rust side).
    payee_declaration: ($) =>
      seq("payee", $._ws, $.string, optional($._trailing_comment)),
    include: ($) =>
      seq("include", $._ws, $.path, optional($._trailing_comment)),

    balance: ($) =>
      seq(
        $.date,
        $._ws,
        "balance",
        $._ws,
        $.account,
        $._ws,
        $.number,
        optional(seq($._ws, "~", $._ws, $.tolerance)),
        $._ws,
        $.commodity_name,
        optional($._trailing_comment),
      ),

    transaction: ($) =>
      seq(
        field("date", $.date),
        optional(seq("=", field("effective_date", $.date))),
        $._ws,
        $.status,
        $._ws,
        $.payee,
        optional(seq($._ws, $.narration)),
        // Tags and links may be freely interleaved, in any order.
        repeat(seq($._ws, choice($.tag, $.link))),
        optional(seq($._ws, $.reference)),
        optional($._trailing_comment),
      ),

    // Metadata line: distinguished from a posting by its trailing `:`
    // right after the (lowercase) key, e.g. `note: some text`, whereas
    // an account never ends with `:` right before whitespace. Used both
    // for directive-level metadata and posting-level metadata (see
    // `directive` and `posting`).
    property: ($) =>
      seq(
        $._ws,
        $.key,
        ":",
        $._ws,
        $.value,
        optional($._trailing_comment),
        $._newline,
      ),

    posting: ($) =>
      seq(
        $._ws,
        $.account,
        optional(seq($._ws, $.amount)),
        optional($._trailing_comment),
        $._newline,
        repeat($._property_or_comment),
      ),

    amount: ($) => seq($.number, $._ws, $.commodity_name),

    status: ($) => choice("*", "!", "?"),
    payee: ($) => $.string,
    narration: ($) => $.string,
    path: ($) => $.string,
    tag: ($) => seq("#", /[A-Za-z0-9_\-]+/),
    link: ($) => seq("^", /[A-Za-z0-9_\-]+/),
    reference: ($) => seq("(", /[^)\r\n]*/, ")"),

    key: ($) => /[a-z_\-]+/,
    // Typed metadata values, close to Beancount: a quoted string, a date
    // or a number (int/decimal, optionally signed).
    value: ($) => choice($.string, $.date, $.number),
    string: ($) => seq('"', /[^"\r\n]*/, '"'),
    date: ($) => /\d{4}-\d{2}-\d{2}/,
    // No spaces allowed in account names (Option A): use `-` or CamelCase
    // instead, e.g. `Actifs:Compte-Joint`. Accounts must start with an
    // uppercase letter so they can never be confused with a lowercase
    // metadata `key` (see `property` above).
    account: ($) => /[A-Z][A-Za-z0-9_\-]*(?::[A-Za-z0-9_\-]+)*/,
    // Beancount-like commodity/currency code: 2 to 24 characters, starting
    // with an uppercase letter, ending with an uppercase letter or digit,
    // and made of uppercase letters, digits, `'`, `.`, `_` or `-` in
    // between.
    commodity_name: ($) => /[A-Z][A-Z0-9'._\-]{0,22}[A-Z0-9]/,
    // Beancount-like number: optional sign, digits optionally grouped by
    // thousands with `,`, optional decimal part.
    number: ($) => /-?\d+(,\d{3})*(\.\d+)?/,
    tolerance: ($) => /\d+(\.\d+)?/,
    comment: ($) => /;[^\r\n]*/,

    // One or more spaces or tabs: used both as an inline separator (which
    // may vary in width, e.g. to align amounts) and as the leading
    // indentation of postings and properties.
    _ws: ($) => /[ \t]+/,
    _newline: ($) => /\r?\n/,
  },
});
