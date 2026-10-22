# Fixtures

Realistic `.pactole` projects used by the test suite and as a reference for
users. They are read by the integration tests of `pactole-storage-fs`,
`pactole-cli` and `pactole-lsp`; those tests skip when the repository is not
checked out (for instance when a crate is tested from a published tarball).

## `journal/`

A complete, valid project: two currencies, a current account, a savings
account and a dollar account, with a salary, rent, everyday expenses and a
short trip, spread over several files tied together by `include` directives.

It is expected to stay **completely clean**: `pactole check` prints `OK`
with no diagnostic, `pactole check --strict` also passes, and every file is
already in canonical form (`pactole fmt` is a no-op). If you change it, run
`pactole fmt --write` on the files you touched.

```sh
pactole check fixtures/journal/ledger.pactole
pactole register fixtures/journal/ledger.pactole --account Actifs:Compte-Joint
```

The documentation walks through it in
[`docs/src/content/docs/tutorials/example-journal.md`](../docs/src/content/docs/tutorials/example-journal.md).

## `journal-partial/`

The deliberately broken sibling: `broken.pactole` holds a directive with an
impossible date and a truncated directive, while the other files are valid.
It pins the partial-analysis behaviour: the business diagnostics of the valid
files are still reported, and the checks that depend on what the lost
directives may have declared or used are suppressed.

```sh
pactole check fixtures/journal-partial/ledger.pactole   # fails, with diagnostics
```
