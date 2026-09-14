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

  rules: {
    source_file: ($) => repeat(choice($.directive, $._newline)),

    directive: ($) =>
      seq($._header, $._newline, repeat(choice($.property, $.posting))),

    _header: ($) =>
      choice(
        $.open,
        $.close,
        $.commodity,
        $.balance,
        $.transaction,
        $.include,
      ),

    open: ($) => seq($.date, " ", "open", " ", $.account),
    close: ($) => seq($.date, " ", "close", " ", $.account),
    commodity: ($) => seq($.date, " ", "commodity", " ", $.commodity_name),
    include: ($) => seq("include", " ", $.path),

    balance: ($) =>
      seq(
        $.date,
        " ",
        "balance",
        " ",
        $.account,
        " ",
        $.number,
        optional(seq(" ", "~", " ", $.tolerance)),
        " ",
        $.commodity_name,
      ),

    transaction: ($) =>
      seq(
        field("date", $.date),
        optional(seq("=", field("effective_date", $.date))),
        " ",
        $.status,
        " ",
        $.payee,
        optional(seq(" ", $.narration)),
        repeat(seq(" ", $.tag)),
        repeat(seq(" ", $.link)),
        optional(seq(" ", $.reference)),
      ),

    // Metadata line: distinguished from a posting by its trailing `:`
    // right after the (lowercase) key, e.g. `note: some text`, whereas
    // an account never ends with `:` right before whitespace.
    property: ($) => seq("  ", $.key, ":", " ", $.value, $._newline),

    posting: ($) =>
      seq(
        "  ",
        $.account,
        optional(seq(" ", $.amount)),
        $._newline,
        repeat($.posting_property),
      ),

    posting_property: ($) =>
      seq("    ", $.key, ":", " ", $.value, $._newline),

    amount: ($) => seq($.number, " ", $.commodity_name),

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
    // metadata `key` (see `property`/`posting_property` above).
    account: ($) => /[A-Z][A-Za-z0-9_\-]*(?::[A-Za-z0-9_\-]+)*/,
    commodity_name: ($) => /[A-Z][A-Z0-9_.\-]*/,
    number: ($) => /-?\d+(\.\d+)?/,
    tolerance: ($) => /\d+(\.\d+)?/,
    comment: ($) => /;[^\r\n]*/,

    _newline: ($) => /\r?\n/,
  },
});
