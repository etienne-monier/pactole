---
title: A complete example journal
description: Walk through the realistic multi-file example shipped with the repository, and see what check, fmt and register report on it.
sidebar:
  order: 2
---

The repository ships a complete, realistic example under
[`fixtures/journal`](https://github.com/etienne-monier/pactole/tree/develop/fixtures/journal).
It is the ledger the test suite uses for its end-to-end tests, and it is a
good model to copy when starting your own: several files tied together by
`include`, two currencies, declarative metadata, balance assertions and the
three transaction statuses.

If you have not installed the CLI yet, follow
[Getting started](/tutorials/getting-started/) first.

## Project layout

```text
fixtures/journal/
├── ledger.pactole          # entry point: includes everything else
├── commodities.pactole     # the currencies the journal uses
├── accounts.pactole        # every account, with a description
├── payees.pactole          # every payee the transactions use
├── pactole.toml            # balance-sheet classification for French roots
└── 2026/
    ├── checking.pactole    # current account, January to March
    ├── savings.pactole     # savings account: transfers and interest
    └── travel.pactole      # dollar account: opening balance and a trip
```

Splitting a journal by year and by account keeps each file small and makes
merge conflicts rare. Only the root file has to be passed to the CLI: the
`include` directives are resolved relative to the including file, so the
whole graph is analyzed together. See
[Split a journal with includes](/how-to/split-a-journal-with-includes/) for
the rules.

## The root file

```pactole
; Example journal used by the Pactole test suite.
;
; It mixes two currencies (EUR and USD), a current account, a savings
; account, a salary, a rent and a few everyday expenses, spread across
; several files tied together by `include` directives.
;
; The whole project is expected to validate without a single diagnostic,
; and to already be in canonical form (`pactole fmt` is a no-op). The
; French account roots are mapped to the balance-sheet categories in
; `pactole.toml`.

include "commodities.pactole"
include "accounts.pactole"
include "payees.pactole"
include "2026/checking.pactole"
include "2026/savings.pactole"
include "2026/travel.pactole"
```

Declarations come first: a commodity or an account must be declared before
the transactions that use it, whichever file they live in.

## Declarations

The two currencies are declared once:

```pactole
2026-01-01 commodity EUR
2026-01-01 commodity USD
```

Each account is opened on the date it becomes available. The dollar account
is restricted to `USD`, and the descriptions show up in editor hovers:

```pactole
2026-01-01 open Actifs:Compte-Joint
  description: "Compte courant"
2026-01-01 open Actifs:Livret
  description: "Livret d'epargne"
2026-01-01 open Actifs:Epargne-USD USD
  description: "Compte en dollars"
```

Payees are declared without a date, because a payee is a global identity
rather than something that starts existing on a given day:

```pactole
payee "Bailleur"
payee "Banque"
payee "Café de la Gare"
; ... one line per payee
payee "Trésor Public"
```

Declaring every account and payee is what makes `pactole check` useful: a
typo in either is reported as an error instead of silently creating a new
account or a new payee.

## A month on the current account

A salary, a rent, groceries and a few smaller expenses. The `fitid` metadata
carries the bank's identifier, which is what makes a later
[`import-ofx`](/how-to/import-ofx-bank-statement/) idempotent:

```pactole
2026-01-05 * "Employeur" "Salaire de janvier"
  fitid: "BANK-2026-01-0001"
  Actifs:Compte-Joint                  3200.00 EUR
  Revenus:Salaire                     -3200.00 EUR

2026-01-05 * "Bailleur" "Loyer de janvier"
  fitid: "BANK-2026-01-0002"
  Actifs:Compte-Joint                  -950.00 EUR
  Depenses:Logement                     950.00 EUR
```

The status in front of the payee records how far the transaction has been
reconciled:

- `*` (cleared): the bank statement confirms it;
- `!` (pending): it happened, but it is not on a statement yet;
- `?` (uncleared): it is entered but unconfirmed.

The fixture uses all three, so the register can be filtered with
`--status`.

Each month ends with a balance assertion, which states what the account
should hold on that date. The `~ 0.01` tolerance absorbs rounding:

```pactole
2026-01-31 balance Actifs:Compte-Joint 1368.30 ~ 0.01 EUR
2026-02-28 balance Actifs:Compte-Joint 2941.75 ~ 0.01 EUR
2026-03-31 balance Actifs:Compte-Joint 4538.55 ~ 0.01 EUR
```

A failed assertion is a validation error, so the assertions double as a
regression test for the journal itself. See
[Reconcile with balance assertions](/how-to/reconcile-with-balance-assertions/)
for the workflow.

## Savings and the dollar account

The savings file adds a monthly transfer and quarterly interest:

```pactole
2026-01-10 * "Banque" "Virement epargne janvier"
  Actifs:Livret                         500.00 EUR
  Actifs:Compte-Joint                  -500.00 EUR

2026-01-28 * "Banque" "Interets du livret"
  Actifs:Livret                           4.17 EUR
  Revenus:Interets                       -4.17 EUR
```

The travel file holds the dollar account. Note that a transaction balances
**per commodity**: there is no exchange rate yet, so a cross-currency
transaction is not representable and each currency moves on its own.

```pactole
2026-02-16 * "Hôtel du Port" "Deux nuits"
  Depenses:Voyage                       240.00 USD
  Actifs:Epargne-USD                   -240.00 USD
```

That limitation, and the plan to lift it, is tracked in the roadmap.

## Running the commands

`check` validates the whole project and prints `OK` when nothing is wrong:

```sh
$ pactole check fixtures/journal/ledger.pactole
OK
```

`--strict` also fails on warnings, such as an unused declaration:

```sh
$ pactole check fixtures/journal/ledger.pactole --strict
OK
```

`fmt` prints the canonical form, and `--write` rewrites the file in place.
Because the fixture is already canonical, this is a no-op:

```sh
$ pactole fmt fixtures/journal/ledger.pactole
# prints the file, byte for byte
```

`register` shows one account's postings with a running balance. On the
current account, the three balance assertions line up with the running
balance:

```sh
$ pactole register fixtures/journal/ledger.pactole --account Actifs:Compte-Joint
2026-01-05  *  Employeur Salaire de janvier              Actifs:Compte-Joint      3200.00 EUR      3200.00 EUR
2026-01-05  *  Bailleur Loyer de janvier                 Actifs:Compte-Joint      -950.00 EUR      2250.00 EUR
2026-01-07  *  Supermarché Courses de la semaine         Actifs:Compte-Joint       -84.30 EUR      2165.70 EUR
...
2026-03-10  *  Banque Virement epargne mars              Actifs:Compte-Joint      -500.00 EUR      4538.55 EUR
```

The number in the last column is the running balance for that commodity, so
the final line matches the last assertion. A dollar account is reported the
same way, in dollars:

```sh
$ pactole register fixtures/journal/ledger.pactole --account Actifs:Epargne-USD
2026-01-01  *  Ouverture Solde d'ouverture               Actifs:Epargne-USD      1000.00 USD      1000.00 USD
2026-02-16  *  Hôtel du Port Deux nuits                  Actifs:Epargne-USD      -240.00 USD       760.00 USD
2026-02-18  *  Bistro du Port Dîner                      Actifs:Epargne-USD       -36.50 USD       723.50 USD
```

The balance sheet uses the `[reports]` mapping in `pactole.toml`:

```sh
$ pactole balance fixtures/journal/ledger.pactole --date 2026-03-31
Balance sheet (as of 2026-03-31)
Assets
  Actifs:Compte-Joint               4538.55 EUR
  Actifs:Epargne-USD                 723.50 USD
  Actifs:Livret                     1512.63 EUR
  Total Assets          6051.18 EUR, 723.50 USD
Liabilities
Equity
  Capitaux:Ouverture                1000.00 USD
  Unclosed result      6051.18 EUR, -276.50 USD
  Total Equity          6051.18 EUR, 723.50 USD
```

## A broken sibling

Next to the clean example lives
[`fixtures/journal-partial`](https://github.com/etienne-monier/pactole/tree/develop/fixtures/journal-partial),
whose `broken.pactole` holds a directive with an impossible date and a
truncated one. It pins a Pactole behaviour worth knowing when one file of a
project is broken: the business diagnostics of the *valid* files are still
reported, while the checks that depend on what the lost directives may have
declared or used are suppressed rather than guessed.

```sh
$ pactole check fixtures/journal-partial/ledger.pactole
fixtures/journal-partial/broken.pactole:9:1: error: [syntax-error] unexpected syntax after a date: `2026-01-15 open`
fixtures/journal-partial/broken.pactole:8:1: error: [lowering-error] parsing error `invalid date `2026-13-45`: input is out of range (expected a valid calendar date in YYYY-MM-DD format)`
fixtures/journal-partial/broken.pactole:8:1: warning: [analysis-incomplete] 2 directives could not be analyzed: the checks that depend on them were skipped
fixtures/journal-partial/valid.pactole:7:14: error: [payee-not-declared] payee `Boulangerie` is not declared (transaction on 2026-01-12)
```

Fixing the broken file is enough to make the warning disappear; the other
files need no change.

## Where to go next

- [The grammar reference](/reference/grammar/) — every directive the example
  uses, and the ones it does not.
- [The diagnostics reference](/reference/diagnostics/) — what each code in
  the output above means.
- [Query with register](/how-to/query-with-register/) — dates, payees, tags
  and status filters.
- [Use the Pactole language server](/how-to/use-the-lsp/) — get the same
  diagnostics inside your editor, with navigation and quick fixes.
