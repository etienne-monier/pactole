/**
 * @file A parser for pactole file
 * @author Etienne Monier
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "pactole",

  extras: ($) => [/\r/, $.comment],

  rules: {
    // TODO: add the actual grammar rules
    source_file: ($) => repeat($.directive),

    directive: ($) =>
      seq(
        $.date,
        /[\ \t]{2,}/,
        "open",
        /[\ \t]{2,}/,
        $.account,
        repeat($.property),
      ),

    property: ($) => seq(/[\ \t]{2,}/, $.key, /[\ \t]{2,}/, $.value),

    key: ($) => /[a-z_\-]+/,
    value: ($) => /[^\n]+/,
    date: ($) => /\d{4}-\d{2}-\d{2}/,
    account: ($) => /[A-Za-z0-9:]+(?: [A-Za-z0-9:]+)*/,
    comment: ($) => /\#.+/,
  },
});
