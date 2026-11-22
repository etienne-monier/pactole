---
title: Value reports in another currency
description: Use dated price directives to convert report amounts without changing the journal.
sidebar:
  order: 9
---

Pactole records account amounts in their original commodities. To display a
report in another commodity, add dated market prices and request valuation.
Valuation changes the report output; it does not rewrite transactions or
account balances.

## Add a market price

A `price` directive gives a direct, dated quote:

```pactole
2026-01-02 price USD 0.92 EUR
2026-01-15 price GBP 1.17 EUR
```

Both commodities must be declared in the journal. Prices must be positive, and
the same directed pair cannot have conflicting values on the same date.

## Request conversion

Pass the target commodity with `--convert-to`:

```sh
pactole balance ledger.pactole --convert-to EUR
pactole income ledger.pactole --convert-to EUR
pactole expenses ledger.pactole --convert-to EUR
pactole register ledger.pactole --convert-to EUR
```

The default valuation date depends on the report. It is normally the report's
end date, or today when the report has no explicit end date. Set it explicitly
with `--at`:

```sh
pactole balance ledger.pactole \
  --date 2026-03-31 --convert-to EUR --at 2026-03-31
```

For `balance`, `--date` controls which postings are included while `--at`
controls which prices are available. They can therefore be different.

## How prices are selected

For every source commodity, Pactole selects the latest direct price on or
before the valuation date. It does not infer either of these conversions:

- the inverse of a quote (`EUR → USD` from `USD → EUR`);
- a multi-step path (`GBP → USD → EUR`).

Add the direct quote you need if a conversion is missing. A missing quote is
reported and the affected amount remains in its original commodity; Pactole
does not silently invent a rate.

## Prices and transaction costs are different

The following annotation balances one transaction using a transaction-specific
weight:

```pactole
  Assets:Brokerage 10 USD @ 0.92 EUR
```

It is not a market price and does not create a reusable quote. A `price`
directive is the reusable dated market data used by reports. See
[Concepts and glossary](/explanation/concepts-and-glossary/) for the distinction
between transaction costs and valuation.

## Exact arithmetic and multiple commodities

Pactole uses exact decimal arithmetic. If a converted product or sum cannot be
represented exactly, the report fails instead of silently rounding it.

If some prices are missing, a converted report can still contain amounts in
several commodities. Conversion is opt-in; without `--convert-to`, reports
always retain their original commodities.
