---
title: Use the web interface
description: Browse balances, income, expenses, the register and diagnostics in a local web interface.
sidebar:
  order: 7
---

Pactole ships a local, read-only web interface. It is served by the same binary
as the CLI and does not require Node.js at runtime.

## Start the server

```sh
pactole serve ledger.pactole
```

The interface is available at <http://127.0.0.1:3000>. Use `--port` or `--bind`
to change the listening address:

```sh
pactole serve ledger.pactole --port 8080
```

The project is reloaded for every request, so saving a journal file and
refreshing the page is enough to see the change. The server does not expose
write operations.

## Views

The navigation drawer gives access to:

- **Dashboard** — current section balances, month-to-date income and expenses,
  a twelve-month balance history, and a monthly expense trend.
- **Balance sheet** — assets, liabilities and equity at a reference date, with
  an optional valuation currency and date.
- **Income statement** — income and expenses over a period, grouped by account,
  payee or tag.
- **Expenses** — spending breakdown grouped by account, payee, tag or month,
  with optional comparison against another period.
- **Register** — filtered postings with their running balance, and a balance
  chart when the filter targets a single account. The account filter offers
  accounts discovered in the balance report.
- **Diagnostics** — syntax, lowering, include, analysis and business
  diagnostics, with their stable code and source location. A source location
  can be copied for use in an editor.

Filters are applied explicitly: edit the fields, then press **Apply**. The
applied filters are kept in the URL, so a view can be bookmarked or shared.
Browser back and forward navigation restores the filters and reloads the view.

## Charts and exact amounts

Charts show one series per commodity and never add different commodities
together. The values displayed in tooltips and in the table view are the exact
decimal strings returned by the API; chart geometry is the only place where a
number is derived from them. Every chart has a table toggle.

## Language and theme

The interface is available in English and French. It starts with the browser
language when it is supported, then English, and remembers an explicit choice
in local storage. The light and dark themes follow the system preference until
you choose one.

## API

The interface is a client of the versioned `/api/v1` JSON API. See the
[web API contract](/reference/web-api/) for the routes, query parameters and
error responses. The frontend bundle is embedded in the `pactole-web` crate, so
installing the CLI is enough to serve it.
