---
title: Why Pactole
description: The ideas behind Pactole, and why plain-text, git-friendly ledgers make sense.
sidebar:
  order: 1
---

Pactole is a plain-text double-entry accounting ledger tool: you keep your
finances in human-readable text files, and the `pactole` tool parses, validates
and reports on them. This page explains the thinking behind that choice.

## Plain text, not a black box

Your ledger is a text file you can read, diff, grep, back up and edit with any
tool. There is no proprietary database and no application to keep running:
the file *is* the data. If Pactole disappeared tomorrow, your ledger would still
be a legible record of your finances.

This is the same philosophy that drives [Beancount](https://beancount.github.io/docs/)
and Ledger-cli, the tools Pactole draws most of its inspiration from.

## Git-friendly by construction

Because a journal is text, it versions naturally with Git. You can review a
commit that adds a transaction, see exactly which lines changed, and revert a
mistake. The canonical formatter (`pactole fmt`) keeps that history clean by
normalizing the layout, so a change to a value produces a one-line diff instead
of a reflow.

## Double-entry, so mistakes surface

Pactole is a **double-entry** system: every transaction moves money between at
least two accounts, and its posting weights must sum to zero per commodity.
`@`/`@@` annotations express transaction-specific conversions; otherwise a
weight is the recorded amount in its own commodity. That rule catches a large
class of errors automatically. On top of it, `pactole check` validates
account lifecycles, declared commodities, declared payees and balance
assertions — see [Double-entry accounting](/explanation/double-entry-accounting/)
for the reasoning, and the
[directives reference](/reference/directives/#validation-invariants) for the
exact rules.

## Where the name comes from

*Pactole* is the ancient name of a small river in Lydia (Asia Minor) whose sands
were said to carry gold washed down from Mount Tmolus — the source of King
Midas's legendary fortune. In French, *"un pactole"* has since become a common
word for a windfall or a small fortune. A fitting name for a tool that helps you
keep track of yours.

## A deliberately small language

Pactole is not trying to cover every Beancount or Ledger feature. It keeps a
small, well-specified grammar (see the
[grammar reference](/reference/grammar/)) and focuses on the common case:
recording day-to-day transactions and checking that they add up. Features like
lot-cost annotations and acquisition tracking are intentionally left out for
now;
[differences from Beancount](/explanation/differences-from-beancount/) lists the
gaps.

## Related

- [Double-entry accounting](/explanation/double-entry-accounting/)
- [Differences from Beancount](/explanation/differences-from-beancount/)
- [Getting started](/tutorials/getting-started/)
