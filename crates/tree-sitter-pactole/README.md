# tree-sitter-pactole

Grammaire [tree-sitter](https://tree-sitter.github.io/tree-sitter/) pour
le langage `.pactole`, avec son parseur C généré et ses bindings Rust.

## Rôle

Cette crate définit la **syntaxe** du langage `.pactole` (voir
[`../../GRAMMAR.md`](../../GRAMMAR.md) pour la spécification complète) et
fournit un parseur exploitable depuis Rust. Elle ne connaît rien au
domaine métier Pactole (comptes, transactions, validation...) : c'est une
brique purement syntaxique, réutilisable par n'importe quel outil basé sur
tree-sitter (éditeurs de texte, autres langages hôtes, etc.).

## Contenu

- `grammar.js` : définition de la grammaire (source de vérité).
- `src/parser.c`, `src/grammar.json`, `src/node-types.json` : parseur C et
  métadonnées générés à partir de `grammar.js` par `tree-sitter generate`.
- `bindings/rust/lib.rs` : binding Rust exposant la grammaire compilée
  (fonction `language()` retournant un `tree_sitter_language::LanguageFn`,
  utilisée par `pactole-syntax`).
- `bindings/rust/build.rs` : compile `src/parser.c` en C via la crate
  `cc` lors du build.
- `queries/highlights.scm` : requêtes de coloration syntaxique pour les
  éditeurs (voir [`../../HELIX.md`](../../HELIX.md) pour l'intégration
  dans Helix).
- `test/` : corpus de tests tree-sitter (`tree-sitter test`).
- `tree-sitter.json` : métadonnées du langage pour l'écosystème
  tree-sitter (`scope: source.pactole`, extension `.pactole`).

## Dépendances / frontières

- Dépendance de build : `cc` (compilation du parseur C).
- Dépendance runtime : `tree-sitter-language`.
- Dépendance de dev : `tree-sitter` (pour les tests du corpus).
- Ne dépend d'aucune autre crate du workspace. C'est la crate la plus en
  amont de l'architecture : `pactole-syntax` en dépend, mais l'inverse
  n'est jamais vrai.

## Modifier la grammaire

Si `grammar.js` est modifié, il faut régénérer le parseur puis propager
les changements aux consommateurs :

```sh
cd crates/tree-sitter-pactole
npx tree-sitter generate
npx tree-sitter test
```

Puis mettre à jour, si nécessaire :

- `queries/highlights.scm` (nouveaux tokens à colorer),
- `crates/pactole-storage-fs/src/parser.rs` et `printer.rs` (nouveaux
  nœuds AST à convertir/formater),
- [`../../GRAMMAR.md`](../../GRAMMAR.md) (spécification).

## Tests / validation

```sh
# Corpus de tests tree-sitter (syntaxe pure)
cd crates/tree-sitter-pactole
npx tree-sitter test

# Compilation et tests Rust du binding
cargo test -p tree-sitter-pactole
```

## Voir aussi

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — architecture globale
  du dépôt et frontières entre crates.
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — spécification du langage
  `.pactole`.
- [`../../HELIX.md`](../../HELIX.md) — intégration de la coloration
  syntaxique dans Helix.
