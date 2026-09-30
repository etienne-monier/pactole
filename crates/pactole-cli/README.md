# pactole-cli

Interface en ligne de commande de Pactole : binaire `pactole`.

## Rôle

Construite avec `clap` (derive), cette crate expose les fonctionnalités
de `pactole-core`/`pactole-storage-fs` via le flux **strict** (voir
[`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3.1) : tout ou rien —
un fichier est soit entièrement parsé/validé avec succès, soit rejeté
avec la première erreur rencontrée (y compris dans un fichier inclus).

Notez que le paquet Cargo s'appelle `pactole-cli`, mais le binaire produit
s'appelle `pactole` (voir le `[[bin]]` de `Cargo.toml`), pour que
`cargo install --path crates/pactole-cli` installe directement une
commande `pactole`.

## Commandes principales

- **`pactole parse <file>`** : parse le fichier et affiche (`Debug`) les
  entrées du journal résultant. Surtout utile pour déboguer la
  grammaire/le parseur.
- **`pactole fmt <file> [--write]`** : reformate un fichier `.pactole`
  dans sa forme canonique, imprimée sur stdout par défaut, ou écrite sur
  place avec `--write`. Accepte `-` comme nom de fichier pour lire depuis
  stdin (toujours imprimé sur stdout dans ce cas).
- **`pactole check <file>`** : parsing syntaxique **puis** validation
  métier complète (`pactole_core::validate_journal`) — comptes ouverts
  avant usage/fermeture, commodités déclarées avant usage, payee de
  chaque transaction déclaré, équilibrage de chaque transaction.
- **`pactole register <file> [filtres]`** : liste les transactions, une
  ligne par posting correspondante, avec un solde courant par commodité.
  Sans filtre, toutes les postings sont listées ; les filtres se
  combinent (`--account` matche aussi les sous-comptes, `--from`/`--to`,
  `--payee`, `--narration` insensibles à la casse, `--tag`, `--status`).

Voir [`../../README.md`](../../README.md) pour des exemples d'usage
complets et l'installation (`cargo install --path crates/pactole-cli`).

## Dépendances / frontières

- `pactole-core` (validation, register), `pactole-storage-fs`
  (`PactoleFileStorage`, `format`) — uniquement leur API stricte,
  historique.
- `clap` (parsing d'arguments), `chrono` (dates de filtre `--from`/`--to`).
- Ne consomme jamais directement `pactole-syntax` ni le flux
  tolérant/positionné (`analysis.rs`/`loader.rs` de
  `pactole-storage-fs`) : ce flux est réservé à `pactole-lsp`.

## Tests / validation

```sh
# Tests d'intégration bout-en-bout de la CLI
cargo test -p pactole-cli --test cli

# Tous les tests de la crate
cargo test -p pactole-cli
cargo clippy -p pactole-cli --all-targets
```

Les tests d'intégration se trouvent dans
`crates/pactole-cli/tests/cli.rs` : ajouter tout nouveau comportement
observable de la CLI (nouvelle commande, nouveau filtre, nouveau code de
sortie) là plutôt que dans `main.rs`.

## Voir aussi

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3.1 et §6 — flux
  strict et détail des commandes.
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — spécification du langage
  `.pactole` manipulé par ces commandes.
- [`../../README.md`](../../README.md) — guide d'installation et
  d'utilisation destiné aux utilisateurs finaux.
