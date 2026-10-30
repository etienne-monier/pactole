---
title: Web API contract
description: Versioned JSON contract for the local read-only web interface.
sidebar:
  order: 6
---

This document defines the HTTP contract for the local web interface planned in
roadmap phase 5. The web server is a presentation adapter over `pactole-app`;
it must not implement accounting rules or expose the details of a particular
storage backend.

The contract is read-only in the MVP. Write capabilities may be advertised by
a future backend, but no write route is part of version 1 of this API.

## Versioning and transport

All endpoints are rooted at `/api/v1` and return JSON:

```text
Content-Type: application/json
```

The API uses additive evolution within a version. Removing a field, changing
its type, or changing the meaning of an existing value requires a new API
version. Clients must use the stable `code` fields rather than matching human-
readable messages.

The server is local by default. Authentication and remote multi-user access are
outside the MVP contract. When it listens on a loopback address, requests whose
`Host` header is not the bind address or `localhost` are rejected with
`host-not-allowed`; binding to a non-loopback address is possible but prints a
warning because the ledger is then exposed without authentication.

## API surface

| Endpoint | Application use case | Purpose |
|---|---|---|
| `GET /api/v1/capabilities` | backend capabilities | Discover supported operations |
| `GET /api/v1/balance` | `BalanceQuery` | Balance sheet at a reference date |
| `GET /api/v1/income` | `IncomeQuery` | Income and expenses over a period |
| `GET /api/v1/expenses` | `ExpensesQuery` | Spending breakdown and comparisons |
| `GET /api/v1/register` | `RegisterQuery` | Filtered postings with running balances |
| `GET /api/v1/diagnostics` | `DiagnosticsQuery` | Current project diagnostics |

The dashboard is a UI composition of these endpoints in the MVP. It displays
the current balance and month-to-date income and expenses without introducing
a second dashboard-specific accounting calculation.

## Common response rules

Report responses use this envelope:

```json
{
  "data": {},
  "diagnostics": []
}
```

`data` contains the result of the requested use case. `diagnostics` contains
warnings and non-blocking diagnostics produced while loading or calculating the
result. Blocking diagnostics are returned as a `ledger-unavailable` error.

Exact decimal amounts are always JSON strings. They must never be serialized
as JSON floating-point numbers:

```json
{
  "commodity": "EUR",
  "number": "1234.50"
}
```

An amount collection is therefore a set of per-commodity values. The API does
not implicitly add amounts in different commodities.

Dates use ISO `YYYY-MM-DD` strings. Absent optional values are represented by
`null`, not by an empty string.

Query parameters are validated strictly: an unknown parameter or a value of the
wrong type is rejected with `invalid-parameter` instead of being ignored.

## Capabilities

### `GET /api/v1/capabilities`

The response describes the backend rather than the current journal:

```json
{
  "capabilities": {
    "read": true,
    "write": false,
    "transactions": false,
    "budgets": false
  }
}
```

The web UI must use this endpoint instead of assuming that every backend is
writable or supports budgets. The local filesystem backend is read-only for the
MVP.

## Report endpoints

### `GET /api/v1/balance`

Builds a balance sheet from `BalanceQuery`.

| Parameter | Type | Meaning |
|---|---|---|
| `as_of` | date | Inclusive reference date; defaults to today |
| `depth` | positive integer | Account depth to display |
| `convert_to` | commodity | Optional report valuation commodity |
| `at` | date | Valuation date; defaults to `as_of` when conversion is requested |
| `include_warnings` | boolean | Include warnings in `diagnostics`; defaults to `false` |

Example:

```text
/api/v1/balance?as_of=2026-10-30&depth=2&convert_to=EUR
```

The `data` value contains the report title, scope, sections, totals and
optional valuation provenance:

```json
{
  "data": {
    "title": "Balance sheet",
    "scope": { "kind": "as_of", "date": "2026-10-30" },
    "sections": [
      {
        "title": "Assets",
        "lines": [
          {
            "label": "Assets:Checking",
            "depth": 2,
            "amounts": [{ "commodity": "EUR", "number": "1250.00" }]
          }
        ],
        "subtotal": [{ "commodity": "EUR", "number": "1250.00" }]
      }
    ],
    "totals": [{ "commodity": "EUR", "number": "1250.00" }],
    "valuation": null
  },
  "diagnostics": []
}
```

### `GET /api/v1/income`

Builds an income statement from `IncomeQuery`.

| Parameter | Type | Meaning |
|---|---|---|
| `from` | date | Inclusive period start; defaults to January 1 of `to`'s year |
| `to` | date | Inclusive period end; defaults to today |
| `by` | `account`, `payee`, `tag` | Grouping; defaults to `account` |
| `depth` | positive integer | Account depth when `by=account` |
| `convert_to` | commodity | Optional report valuation commodity |
| `at` | date | Valuation date; defaults to `to` when conversion is requested |
| `include_warnings` | boolean | Include warnings; defaults to `false` |

The scope is represented as:

```json
{
  "kind": "over",
  "from": "2026-10-01",
  "to": "2026-10-30"
}
```

### `GET /api/v1/expenses`

Builds a spending breakdown from `ExpensesQuery`.

| Parameter | Type | Meaning |
|---|---|---|
| `from`, `to` | date | Inclusive report period |
| `by` | `account`, `payee`, `tag`, `month` | Expense grouping; defaults to `account` |
| `depth` | positive integer | Account depth when `by=account` |
| `uncategorized` | boolean | Include uncategorized transactions |
| `inactive` | boolean | Include inactive expense accounts |
| `compare` | `previous`, `absolute` | Request a comparison period |
| `compare_from`, `compare_to` | date | Explicit comparison period |
| `diff` | `absolute`, `percentage` | Difference representation; defaults to `absolute` |
| `reverse_diff` | boolean | Compute previous minus current; defaults to `false` |
| `convert_to` | commodity | Optional report valuation commodity |
| `at` | date | Valuation date |
| `include_warnings` | boolean | Include warnings; defaults to `false` |

The application-layer constraints apply unchanged:

- monthly grouping cannot be compared;
- comparison cannot be combined with `uncategorized` or `inactive`;
- compared periods must have the same length;
- `depth` applies only to account grouping.

The response contains the current report and, when requested, a comparison:

```json
{
  "data": {
    "current": {},
    "comparison": null
  },
  "diagnostics": []
}
```

### `GET /api/v1/register`

Lists matching postings and their running balances.

| Parameter | Type | Meaning |
|---|---|---|
| `account` | string | Account or account prefix |
| `from`, `to` | date | Inclusive transaction date bounds |
| `payee` | string | Payee substring filter |
| `narration` | string | Narration substring filter |
| `tag` | string | Transaction tag |
| `status` | `cleared`, `pending`, `uncleared` | Transaction status |
| `convert_to` | commodity | Optional market-price valuation |
| `at` | date | Valuation date |
| `include_warnings` | boolean | Include warnings; defaults to `false` |

An empty result is successful and returns an empty `entries` array. The result
also includes `valuation` when conversion was requested.

## Diagnostics

### `GET /api/v1/diagnostics`

Returns diagnostics from the analyzed project.

| Parameter | Type | Meaning |
|---|---|---|
| `include_warnings` | boolean | Include warnings; defaults to `false` |

Example:

```json
{
  "data": {
    "diagnostics": [
      {
        "severity": "error",
        "origin": "business",
        "code": "account-not-open",
        "message": "account Assets:Checking is not open",
        "source": "journal.pactole",
        "span": {
          "start": { "line": 12, "column": 4 },
          "end": { "line": 12, "column": 19 }
        }
      }
    ]
  },
  "diagnostics": []
}
```

Diagnostic codes are the stable values from `DiagnosticCode::as_str()`. The
allowed origins are `syntax`, `lowering`, `include`, `analysis` and `business`;
the allowed severities are `error` and `warning`.

Source identifiers are logical project-relative names. Absolute filesystem
paths, loader types, Tree-sitter nodes and other storage details are not part
of the web contract. Lines and columns in HTTP responses are one-based and
refer to the source file identified by `source`. The current application
boundary exposes byte columns within UTF-8 lines, so spans include
`column_encoding: "utf8-byte"`; a future backend may provide a richer source
mapping without changing diagnostic codes.

## Error responses

Every failed request uses the same top-level shape:

```json
{
  "error": {
    "code": "invalid-query",
    "message": "comparison is not supported when grouping expenses by month",
    "details": null
  }
}
```

Clients must branch on `error.code`; `message` is for display and diagnostics.

| HTTP status | Error code | Meaning |
|---:|---|---|
| `400` | `invalid-parameter` | A parameter has an invalid syntax or value |
| `400` | `invalid-query` | Parameters form an unsupported combination |
| `400` | `undeclared-valuation-commodity` | The requested valuation commodity is absent |
| `400` | `report-error` | The report could not be computed (invalid period, valuation overflow) |
| `403` | `host-not-allowed` | The request `Host` header does not match the loopback server |
| `404` | `not-found` | The route or requested resource does not exist |
| `405` | `method-not-allowed` | The HTTP method is not supported |
| `422` | `ledger-unavailable` | Error diagnostics prevent report construction |
| `500` | `backend-error` | The backend could not load its snapshot |
| `500` | `application-error` | An unexpected application failure occurred |
| `501` | `capability-unavailable` | The backend lacks a required capability |

`ledger-unavailable` includes the blocking diagnostics in `details`:

```json
{
  "error": {
    "code": "ledger-unavailable",
    "message": "the ledger is unavailable because its diagnostics contain errors",
    "details": {
      "diagnostics": []
    }
  }
}
```

## Contract verification

The web adapter must verify the following contract cases with the shared
`fixtures/journal` project:

- capabilities expose a read-only backend;
- balance, income, expenses and register results contain exact decimal strings;
- multi-commodity results keep amounts separate;
- valuation exposes its target, date, used prices and missing conversions;
- expense comparison preserves the application-layer validation rules;
- diagnostics expose stable codes, origins, severities and project-relative sources;
- missing includes and partial analysis return structured diagnostics;
- malformed queries return the common error envelope;
- web JSON has the same report semantics as the corresponding CLI JSON output.

These tests belong to the HTTP adapter in the later server phase. They are
listed here so that changes to the API contract remain deliberate and
backwards-compatible.
