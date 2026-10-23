# Pactole Roadmap

This roadmap describes where Pactole stands today, the product directions it
is heading towards, and the concrete tasks to implement them.

Priorities are classified as follows:

- **P0**: foundation required for product reliability;
- **P1**: high user value, to be delivered next;
- **P2**: important improvements that depend on a more stable foundation;
- **P3**: exploratory or advanced features.

Tasks are intentionally small and should ideally result in one focused PR with
targeted tests. Checked items describe the known state when this document was
written; update this list as features evolve.

This document is the single source of truth for the planned work. The
delivered history lives in `CHANGELOG.md` and in the git history, not here:
when an item ships, tick its box and record the change in the changelog.

The five product directions are described in
[Product directions](#product-directions). The implementation phases below are
ordered by dependency, not by size: a phase unlocks the next one.

---

## Where Pactole stands today

Pactole 0.3.0 is a working plain-text, double-entry accounting tool. The
foundation is in place and should be preserved:

- **A clean crate architecture** with enforced boundaries: `pactole-core` is
  pure domain logic with no I/O and no tree-sitter, `pactole-syntax` is
  purely syntactic and tolerant, `pactole-config` is a leaf crate owning the
  `pactole.toml` schema, `pactole-storage-fs` lowers a CST into domain models,
  `pactole-import` is a zero-I/O edge adapter, and the `pactole-cli` and
  `pactole-lsp` front-ends only consume the public APIs of those crates.
- **A unified project analysis**: `analyze_file_with_loader` resolves
  `include` directives, detects cycles, deduplicates diamonds, and returns
  positioned diagnostics. The CLI and the LSP share it.
- **Business validation**: account lifecycle, declared payees and commodities,
  unused declarations, transaction balancing, and balance assertions — all
  reported as positioned diagnostics with configurable severity.
- **An editor integration** with diagnostics, completion, hover, go-to-definition,
  find-references, rename, and declaration quick fixes.
- **A CLI** with `parse`, `fmt`, `check`, `register`, `init`, `import-ofx` and
  `completions`.
- **An OFX import pipeline** handling encodings, multiple accounts, and
  idempotence through FITIDs.

The main gaps, in order of impact:

1. **Reporting.** The only report is the register. There is no shared reporting
   API, no balance sheet, no income statement, no spending breakdown. This
   blocks the CLI, the web interface, and budgets alike.
2. **Multi-currency.** Postings are single-commodity and transactions must
   balance *per commodity*. A transaction that exchanges currencies therefore
   cannot balance today, and there is no price, cost, or conversion concept.
3. **Grammar coverage.** `pad`, `price`, `note`, `document`, tags and
   `custom` are missing, and strings cannot contain a double quote.
4. **Budgeting.** Nothing exists yet, in any form.
5. **Debt in the current code.** Phase 0 is complete: a broken file no longer
   suspends business validation for the whole project, diagnostic codes are a
   typed catalogue, and the test suite pins the partial-analysis, formatting
   and editor-positioning contracts with realistic fixtures and property
   tests. What remains is the reporting layer that Phase 1 introduces.

---

## Product directions

### 1. Multi-currency support

Pactole must be able to record a journal that mixes currencies without
lying about the balances. Today it cannot: a transaction exchanging EUR for
USD cannot balance, because balancing is per commodity and no price exists.

This direction is the deepest one, because it touches the domain model, the
grammar, the balancing rules, the reports, and the import pipeline. It is
also the one that most blocks the other directions: budgets and reports would
have to be redone once currency conversion exists.

Target model:

- a `price` directive carrying a dated conversion rate between two commodities;
- `@` (unit price) and `@@` (total price) annotations on postings;
- balancing computed on the converted weight, with explicit rounding and
  tolerance rules;
- a `Price` and `Cost` type distinct from `Amount`;
- reports able to express a report currency and a valuation date.

Deliberately out of scope: lot tracking (`{cost}`), and the notion of a
security or a market price feed.

### 2. Grammar convergence with Beancount

Pactole is inspired by Beancount but is deliberately smaller. The goal here is
not to clone Beancount: it is to remove the friction that makes migrating
from Beancount painful, one directive at a time, in order of real usage.

Each directive must arrive as a complete unit: grammar, tree-sitter
regeneration, corpus tests, lowering, canonical printing, LSP support
(hover, completion, symbols, quick fixes), and documentation.

Out of scope: `event`, `query`, `plugin`, and lot annotations.

### 3. Envelope budgeting

Budgeting is what turns an accounting tool into a daily companion. Two models
are possible, and the choice must be made before any code is written:

- **Budget over the ledger** (recommended first step): a declarative budget
  per account and period, compared against the actual spending already
  recorded. This is a report plus a directive, and it reuses the double-entry
  model as-is.
- **True envelopes** (deferred): dedicated accounts or a dedicated mechanism
  to hold funds, with transfers between envelopes and a "carried over"
  balance. This is closer to YNAB, but it either pollutes the account tree or
  requires a new accounting concept.

Recommendation: ship the first model, learn from real usage, and only then
decide whether envelopes need their own model.

### 4. Web server for reports

A read-only web interface would make the reports pleasant to consult, and
would let charts and period comparisons live outside the terminal.

This direction is a *facade over the reporting API*, not a product of its
own. It must come after the reports exist and are stable, and it must not
introduce its own accounting logic. Scope: local, read-only, no write-back to
the ledger.

### 5. Complete the CLI reports

The CLI is the primary interface today. It should be able to answer the
questions a user actually asks: what do I have, what did I earn and spend,
where did my money go, and am I on budget.

This direction is the most visible and the cheapest, but it must be built on
a shared reporting API rather than on ad-hoc commands, so the web interface
and the budget reports can reuse it.

---

## Phase 0 — Consolidate the foundation (P0)

*Goal: make the roadmap trustworthy and the diagnostics stable before adding
features.*

### 0.1 — Refresh the project documentation

- [x] Update `differences-from-beancount.md` whenever a directive is added.
- [x] Keep the user documentation and the CLI `--help` in sync with the
  actual feature set.
- [x] Keep `AGENTS.md` aligned with the public API of the crates.

### 0.2 — Type the diagnostic codes

- [x] Introduce a `DiagnosticCode` enum in `pactole-core` (or
  `pactole-storage-fs`, depending on where the codes are produced) covering
  syntax, lowering, include, account lifecycle, payee, commodity, balancing
  and balance-assertion issues.
- [x] Convert the code to text only at the CLI and LSP boundaries.
- [x] Document the catalogue and declare the codes as a stable public API.
- [x] Replace every free-form `Option<String>` code in the tree.

### 0.3 — Support partial analyses

- [x] When a file fails to parse or to lower, still validate the reliable
  entries of the other files.
- [x] Distinguish the diagnostics that depend on the missing data and
  suppress them, instead of losing all business diagnostics.
- [x] Represent a partial analysis explicitly in the public API.
- [x] Add regression tests for a broken file next to a valid one.

### 0.4 — Strengthen the test suite

- [x] Add realistic multi-file journal fixtures (bank statements, savings,
  salary, rent, a few currencies).
- [x] Add property tests for balancing and for the idempotence of the
  canonical formatter.
- [x] Add end-to-end tests converting business diagnostics to UTF-16 LSP
  diagnostics, including non-ASCII text.
- [x] Keep `cargo fmt --check`, `cargo clippy --workspace --all-targets` and
  `cargo test --workspace` green in CI.

---

## Phase 1 — Reporting API and CLI reports (P1, v0.4)

*Goal: a shared, render-agnostic reporting layer, and the first complete
reports on top of it.*

### 1.1 — Introduce the reporting API

- [x] Define a `Period` and a reference-date abstraction (inclusive bounds).
- [x] Define result structures that do not depend on any text rendering.
- [x] Keep the commodity on every report line, and define how a multi-commodity
  total is represented.
- [x] Aggregate into parent accounts (a configurable depth, or the full prefix
  hierarchy).
- [x] Provide text, JSON and CSV renderings of the same result.
- [x] Add tests for inclusive dates, parent accounts, and multiple commodities.

*Placement: `pactole-core` (pure domain logic). The CLI and any future web
front-end only render the results.*

### 1.2 — Implement the balance sheet

- [x] Compute account balances at a given date.
- [x] Group accounts by hierarchical prefix.
- [x] Separate assets, liabilities and equity, using a documented convention
  with an explicit configuration override.
- [x] Add the `pactole balance` command, with `--date` and `--format`.
- [x] Add fixtures and compare the output against known journals.

### 1.3 — Implement the income statement

- [ ] Compute income and expenses over a period.
- [ ] Group by account, payee and tag.
- [ ] Add `pactole income --from --to`, with `--format`.
- [ ] Document the multi-commodity behaviour (per-commodity totals, no
  implicit conversion until Phase 2).

### 1.4 — Implement spending breakdowns

- [ ] `pactole expenses --by account|payee|tag|month`.
- [ ] Report uncategorized transactions and inactive accounts.
- [ ] Compare two periods (same length, relative or absolute difference).

### 1.5 — Improve the CLI output

- [ ] Add a `--format json` option to `check`, and to every report.
- [ ] Add a command to display the `include` graph, and to report cycles and
  files included more than once.
- [ ] Report `.pactole` files unreachable from the root journal.
- [ ] Keep the text output stable for scripts and CI.

---

## Phase 2 — Multi-currency (P1, v0.5)

*Goal: a journal that mixes currencies balances correctly, and reports that can
be valued.*

### 2.1 — Specify the model first

- [ ] Document the current commodity semantics precisely.
- [ ] Specify `Amount`, `Price`, `Cost` and `Value` as distinct concepts, and
  decide what actually goes into the domain model.
- [ ] Define the rounding and tolerance rules for a converted transaction.
- [ ] Define the behaviour when a required price is missing.
- [ ] Write the specification in `GRAMMAR.md` before touching the grammar.

### 2.2 — Add the `price` directive

- [ ] Extend the grammar and regenerate the tree-sitter parser.
- [ ] Lower the directive into a domain model, and print it canonically.
- [ ] Validate: unknown commodities, contradictory prices on the same day,
  prices before the first use.
- [ ] Add LSP support (hover, completion, navigation).

### 2.3 — Add price annotations on postings

- [ ] Support `@ unit_price` and `@@ total_price` on postings.
- [ ] Infer the missing side of the price pair, as for implicit amounts.
- [ ] Keep the canonical printing deterministic.

### 2.4 — Balance on converted weights

- [ ] Compute the converted weight of each posting, and balance the
  transaction on it.
- [ ] Report a dedicated diagnostic for an unbalanced multi-commodity
  transaction, and for a missing price.
- [ ] Apply the documented tolerance and rounding rules.
- [ ] Keep single-commodity transactions byte-for-byte identical to today.

### 2.5 — Value the reports

- [ ] Add a report currency and a valuation date to the reporting API.
- [ ] Report the price used for each conversion, and flag missing prices.
- [ ] Add `--convert-to` and `--at` to the report commands.
- [ ] Keep the unconverted (per-commodity) view available.

### 2.6 — Follow through in the rest of the toolchain

- [ ] Import OFX statements in their own currency (`CURDEF`, `ORIGCURRENCY`),
  and convert them with the journal's prices.
- [ ] Show the balance of a foreign-currency account in hover.
- [ ] Handle commodity restrictions in `open` directives with a price.

---

## Phase 3 — Grammar convergence with Beancount (P2, v0.6)

*Goal: remove the friction of migrating from Beancount, one directive at a
time, in order of real usage.*

Each directive is a complete unit: grammar, tree-sitter regeneration, corpus
tests, lowering, canonical printing, LSP support, documentation.

### 3.1 — Escape sequences in strings

- [ ] Allow `\"` inside a quoted string, without breaking the current
  "a string cannot contain a double quote" rule elsewhere.
- [ ] Define the escaping rules for the payee, the narration, and the
  metadata values.
- [ ] Cover the round trip in the formatter tests.

### 3.2 — `pad`

- [ ] Add the `pad` directive, with the account and the amount to pad to.
- [ ] Generate the corresponding opening entry.
- [ ] Validate the padded account and the date ordering.

### 3.3 — `note` and `document`

- [ ] Add the two directives, as purely declarative entries.
- [ ] Surface them in the LSP (document symbols, navigation).
- [ ] Keep them out of the reports.

### 3.4 — `pushtag` and `poptag`

- [ ] Add the two directives, and make the tags visible in the reports.
- [ ] Decide how they interact with the tags already carried by transactions.

### 3.5 — `option` and `custom`

- [ ] Add `option`, with a documented set of known keys.
- [ ] Add `custom`, as an extension point carrying structured metadata.
- [ ] Decide whether `custom` becomes the carrier for the budget directive
  (Phase 4).

### 3.6 — Explicitly out of scope

- [ ] `event`, `query`, `plugin`: rejected with an explicit diagnostic, not
  a silent parse error.

---

## Phase 4 — Envelope budgeting (P2, v0.7)

*Goal: answer "am I on budget?" from the ledger, without a second accounting
model.*

### 4.1 — Decide the model

- [ ] Write a short design note comparing the two models, and pick one.
- [ ] Define the period semantics (monthly, annual, and what happens to an
  unused amount).
- [ ] Define the interaction with a partially-elapsed month.

### 4.2 — Declare budgets

- [ ] Add the budget declaration (a `budget` directive, or `custom "budget"`).
- [ ] Support an amount, a commodity, a period and a target account.
- [ ] Validate the target account, the commodity, and the period.
- [ ] Add LSP support.

### 4.3 — Report budgets

- [ ] `pactole budget [--month YYYY-MM]`: planned, actual, remaining, and the
  deviation.
- [ ] Show the actual spending of the budgeted account, restricted to the
  relevant postings.
- [ ] Report the budget as a warning (or an error, when configured) when it is
  exceeded.
- [ ] Reuse the Phase 1 reporting API; do not write a second accounting path.

### 4.4 — Envelopes, if the design note asks for them

- [ ] Define how an envelope is represented, without polluting the account
  tree if possible.
- [ ] Define the transfers between envelopes, and the carried-over balance.
- [ ] Report the envelope balances over time.

---

## Phase 5 — Web server for reports (P3)

*Goal: a pleasant, local, read-only view of the reports.*

### 5.1 — Define the use cases

- [ ] List what the web interface must do that the CLI and the LSP do not
  (overview, charts, period comparison, sharing).
- [ ] Confirm the reports it needs are all available from Phase 1.

### 5.2 — Build the server

- [ ] New crate `pactole-web`, depending only on the public APIs of
  `pactole-core`, `pactole-storage-fs` and `pactole-config`.
- [ ] Reuse the project analysis and the reporting API; no accounting logic of
  its own.
- [ ] Serve the reports as JSON, and the UI as static assets.
- [ ] Bind to `127.0.0.1` by default, read-only, with no write-back to the
  ledger.
- [ ] Reload when the ledger files change on disk.

### 5.3 — Build the interface

- [ ] Dashboard: current balances, month-to-date income and expenses.
- [ ] Balance sheet, income statement, spending breakdowns.
- [ ] Budget view, once Phase 4 exists.
- [ ] Charts over time, and period comparison.
- [ ] Ship the front-end as static assets, without a Node build chain.

---

## Continuous work

These items improve the existing product without waiting for a phase:

- [ ] Expose formatting through the LSP (`textDocument/formatting`), by
  calling the canonical formatter.
- [ ] Implement `textDocument/documentSymbol`.
- [ ] Build a positioned symbol index (accounts, payees, commodities, tags,
  links), shared by completion, hover, navigation and rename.
- [ ] Only suggest accounts that are usable on the current date, and never
  suggest a closed account after its closing date.
- [ ] Show the current balance of an account in hover.
- [ ] Add a quick fix inserting an inferred amount when a transaction has a
  single possible implicit posting.
- [ ] Add a configurable CSV import, reusing the `ImportRecord` pipeline and
  the rule engine, with a `--preview` mode and a documented deduplication key.
- [ ] Improve import idempotence and provenance: report imported, skipped and
  ambiguous transactions separately.
- [ ] Add an analysis cache, only after measuring the analysis time on real
  journals; keep it outside `pactole-storage-fs` if that preserves its
  current responsibility.
- [ ] Extend the editor integrations: a tested Helix configuration, Neovim
  documentation, and snippets for the common directives.
- [ ] Test the LSP against several clients, especially UTF-16 ones.
- [ ] Introduce incremental LSP synchronization only if full synchronization
  becomes a measured problem.
- [ ] Keep `pactole-core` free of I/O, `pactole-syntax` free of domain
  models, and `pactole-import` free of file-format knowledge.

---

## Explicitly out of scope

- Lot tracking and securities (`{cost}`, `{}`).
- A graph of "value" independent of a price.
- A hosted or multi-user service; the web interface stays local.
- Writing back to the ledger from the web interface.
- Beancount's `event`, `query` and `plugin` directives.
- A graphical desktop application.

---

## Recommended delivery order

1. **Phase 0** — refresh the documentation, type the diagnostic codes, support
   partial analyses, strengthen the tests.
2. **Phase 1** — the reporting API, then the balance sheet, the income
   statement, the spending breakdowns and the CLI output options.
3. **Phase 2** — the multi-currency model, then the `price` directive, the
   price annotations, the converted balancing, and the valued reports.
4. **Phase 3** — the grammar, selectively, in order of real usage.
5. **Phase 4** — the design note, then the budget declaration and the budget
   report.
6. **Phase 5** — the web server, last, as a facade over stable reports.

This order maximizes user value while reusing the foundations already in
place: tolerant analysis, spans, include resolution, the domain model, the
canonical formatter, the import pipeline, and the layered configuration.
