# pactole-core

Domaine métier pur de Pactole : modèles, invariants de validation et
calculs financiers.

## Rôle

`pactole-core` contient uniquement de la logique de domaine : **aucune
E/S, aucune dépendance à tree-sitter**. Elle ne doit jamais dépendre de
`pactole-storage-fs`, `pactole-syntax`, `tree-sitter`, ni d'aucune
opération de système de fichiers. Toute conversion depuis un fichier
`.pactole` (syntaxe concrète → modèles) est de la responsabilité de
`pactole-storage-fs`.

## API principale

Réexportée depuis `src/lib.rs` :

- **Modèles** (`models/`) : `Journal`, `Entry`, `Transaction`, `Posting`,
  `Amount`, `Balance`, `Open`, `Close`, `Commodity`, `Payee`,
  `TransactionStatus`, `Metadata`, ainsi que des newtypes validées
  (`AccountName`, `CommodityName`, `MetadataKey`).
- **`validate_journal(journal: Journal) -> Result<Journal, ValidationError>`**
  (`validation.rs`) : consomme le `Journal` en entrée et vérifie les
  invariants métier — ordre chronologique des entrées, cycle de vie des
  comptes (ouverts avant usage/fermeture, non utilisés après fermeture),
  commodités déclarées avant usage, payees déclarés avant usage dans une
  transaction, équilibrage de chaque transaction par commodité. En cas de
  succès, renvoie le `Journal` (trié chronologiquement et avec les
  transactions auto-équilibrées) ; en cas d'échec, renvoie une
  `ValidationError`.
- **`register(journal: &Journal, filter: &RegisterFilter) -> Vec<RegisterEntry>`**
  (`register.rs`) : calcule un rapport "register" (solde courant par
  commodité), filtrable par compte (avec sous-comptes), plage de dates,
  payee, narration, tag, statut.
- **`ReadableStorage`** (`traits.rs`) : trait d'abstraction de stockage,
  implémenté par `pactole-storage-fs::PactoleFileStorage` (et destiné à
  l'être par une future crate de stockage base de données).
- **`ModelError`, `ValidationError`** (`errors.rs`) : erreurs structurées
  via `thiserror`.

## Dépendances / frontières

- `chrono` (dates), `rust_decimal` (arithmétique financière — jamais de
  `f32`/`f64` pour des montants), `thiserror` (erreurs structurées).
- Ne dépend d'aucune autre crate du workspace.

## Tests / validation

```sh
cargo test -p pactole-core
cargo clippy -p pactole-core --all-targets
```

Les tests unitaires de logique de domaine et de validation vivent dans
cette crate (voir les modules `validation.rs`, `register.rs`, `models/`).

## Voir aussi

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — rôle de
  `pactole-core` dans l'architecture globale et frontières avec les
  autres crates.
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — spécification du langage dont
  ces modèles et invariants sont l'expression en Rust.
