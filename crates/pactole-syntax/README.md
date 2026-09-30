# pactole-syntax

Analyse syntaxique **tolérante** de texte source `.pactole` : positions,
diagnostics, sans jamais d'échec irrécupérable.

## Rôle

`pactole-syntax` se situe entre la grammaire tree-sitter brute
(`tree-sitter-pactole`) et les consommateurs de plus haut niveau comme
`pactole-storage-fs` ou `pactole-lsp`. Elle ne fait aucune E/S et ne
dépend jamais de `pactole-core` : elle ne manipule que l'arbre syntaxique
concret (CST) et ses diagnostics, jamais de modèle de domaine. Le parsing
qu'elle fournit réussit **toujours**, même sur un texte invalide : les
erreurs sont rapportées comme des diagnostics positionnés plutôt que
comme des échecs bloquants.

## API principale

Réexportée depuis `src/lib.rs` :

- **`span`** : `Point` (offset en octets + ligne/colonne 0-indexées) et
  `Span` (paire de `Point` en intervalle semi-ouvert), avec
  `Span::byte_range()` pour découper le texte source.
- **`diagnostics`** : `SyntaxDiagnostic` (un `DiagnosticKind`, un `Span`,
  un message) et `DiagnosticKind` (`Error` pour les nœuds `ERROR`
  tree-sitter, `Missing` pour les nœuds "missing").
- **`document`** :
  - `parse_document(source: &str) -> ParsedDocument` — point d'entrée du
    parsing tolérant ; `analyze` est un alias de cette fonction ;
  - `ParsedDocument` — source + `Tree` tree-sitter + diagnostics
    collectés ; `ParsedDocument::into_result()` offre une conversion
    stricte en `Result<ParsedDocument, Vec<SyntaxDiagnostic>>` pour les
    consommateurs (comme `pactole-storage-fs::parser`/`printer`) qui ne
    veulent avancer que sur un document syntaxiquement propre ;
  - `collect_error_nodes(tree: &Tree) -> Vec<SyntaxDiagnostic>` — parcourt
    un `Tree` tree-sitter et collecte les nœuds `ERROR`/`MISSING`.

## Dépendances / frontières

- `tree-sitter`, `tree-sitter-pactole` (grammaire compilée).
- Ne dépend jamais de `pactole-core` : cette crate reste purement
  syntaxique par construction, socle partagé pour tout futur outillage
  ayant besoin de diagnostics positionnés sur un texte potentiellement
  invalide, sans dupliquer la plomberie tree-sitter à chaque consommateur.

## Tests / validation

```sh
cargo test -p pactole-syntax
cargo clippy -p pactole-syntax --all-targets
```

Les tests unitaires (dans `src/lib.rs`) couvrent le parsing valide/invalide,
la conversion stricte via `into_result`, et la cohérence entre
`collect_error_nodes` et les diagnostics d'un `ParsedDocument`.

## Voir aussi

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — rôle de
  `pactole-syntax` dans le flux tolérant/positionné et ses frontières avec
  `pactole-storage-fs` et `pactole-lsp`.
