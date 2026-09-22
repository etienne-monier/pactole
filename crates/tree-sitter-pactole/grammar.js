/**
 * @file A parser for pactole file
 * @author Etienne Monier
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "pactole",

  extras: ($) => [$.comment],

  // The grammar cannot locally tell, right after a posting's account line,
  // whether the following indented line starts this posting's own
  // metadata (`property`) or is the next posting in the directive: both
  // start with the same flexible `_ws` token. Let the GLR parser explore
  // both branches; they always resolve once the `key ":"` vs `account`
  // shape is seen.
  conflicts: ($) => [[$.posting]],

  rules: {
    source_file: ($) => repeat(choice($.directive, $._newline)),

    // Directive-level metadata (`property`) must come before any posting:
    // this keeps the grammar unambiguous while allowing a flexible amount
    // of leading whitespace (see `_ws`), since a `key: value` line right
    // after a posting is otherwise indistinguishable from one belonging to
    // the directive itself.
    directive: ($) =>
      seq($._header, $._newline, repeat($.property), repeat($.posting)),

    _header: ($) =>
      choice(
        $.open,
        $.close,
        $.commodity,
        $.balance,
        $.transaction,
        $.include,
      ),

    open: ($) => seq($.date, $._ws, "open", $._ws, $.account),
    close: ($) => seq($.date, $._ws, "close", $._ws, $.account),
    commodity: ($) =>
      seq($.date, $._ws, "commodity", $._ws, $.commodity_name),
    include: ($) => seq("include", $._ws, $.path),

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
      ),

    // Metadata line: distinguished from a posting by its trailing `:`
    // right after the (lowercase) key, e.g. `note: some text`, whereas
    // an account never ends with `:` right before whitespace. Used both
    // for directive-level metadata and posting-level metadata (see
    // `directive` and `posting`).
    property: ($) => seq($._ws, $.key, ":", $._ws, $.value, $._newline),

    posting: ($) =>
      seq(
        $._ws,
        $.account,
        optional(seq($._ws, $.amount)),
        $._newline,
        repeat($.property),
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
