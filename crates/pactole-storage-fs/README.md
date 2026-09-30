# pactole-storage-fs

Stockage `.pactole` basé fichiers : parseur strict, formateur canonique,
et analyse tolérante multi-fichiers pour l'outillage.

## Rôle

`pactole-storage-fs` est le seul point de jonction entre le monde
syntaxique (tree-sitter / `pactole-syntax`) et le monde domaine
(`pactole-core`). Elle expose deux flux distincts (voir
[`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3 pour le détail) :

- un **flux strict**, historique et inchangé, tout-ou-rien : lecture
  fichier → parsing → `pactole_core::Journal` unique et aplati (fichiers
  inclus fusionnés), utilisé par `pactole-cli` ;
- un **flux tolérant/positionné**, additif, qui ne fait jamais échouer
  l'analyse entière et préserve la provenance par fichier, utilisé par
  `pactole-lsp`.

## API principale

Réexportée depuis `src/lib.rs` :

- **Flux strict** :
  - `PactoleFileStorage` : implémente `pactole_core::ReadableStorage`
    pour un fichier local. `TryFrom<PathBuf>` lit le fichier ;
    `parse()` le convertit en `Journal` (résolvant récursivement les
    `include`) ; `journal()` renvoie le `Journal` obtenu.
  - `format(source: &str) -> Result<String, PactoleFsStorageError>`
    (`printer.rs`) : reformate du texte `.pactole` dans sa mise en page
    canonique.
  - `PactoleFsStorageError` (`errors.rs`) : erreurs structurées
    (`thiserror`).
- **Flux tolérant/positionné** (`analysis.rs`) :
  - `analyze_file(source: &str) -> ParsedFile` : analyse un seul fichier
    sans résoudre ses `include` ; chaque directive est abaissée
    indépendamment, produisant soit un `ParsedEntry { span, entry }` soit
    un `LoweringDiagnostic { span, message }` sans jamais abandonner tout
    le fichier ; `ParsedInclude { span, path }` détecte les `include`
    (chemin brut, non résolu, ni lu ni parsé ici) ;
    `ParsedFile::syntax_diagnostics()` expose les `SyntaxDiagnostic`s
    sous-jacents de `pactole-syntax`.
  - `analyze_file_with_loader(entry_path, loader: &dyn SourceLoader) -> Result<AnalyzedProject, ...>`
    : suit récursivement les `include` via un `SourceLoader`, produisant
    un `AnalyzedProject` (chaque fichier gardant son propre
    `AnalyzedFile { path, file }`) et une liste d'`IncludeIssue` pour les
    problèmes d'inclusion (chemin non résolvable, cycle, cible non
    chargeable) — jamais un échec global, sauf si `entry_path` lui-même ne
    peut être chargé.
- **Chargement de source** (`loader.rs`) :
  - `SourceLoader` : trait minimal `fn load(&self, path: &Path) -> Result<String, SourceLoadError>`.
  - `FsSourceLoader` : lit de vrais fichiers du système de fichiers.
  - `InMemorySourceLoader` : sert du contenu depuis une table en mémoire
    (utile pour les tests et préfigure un futur chargeur adossé à des
    buffers d'éditeur).
  - `SourceLoadError` : erreur de chargement.

## Dépendances / frontières

- `pactole-core` (modèles de domaine), `pactole-syntax` (parsing
  tolérant, spans, diagnostics), `tree-sitter` (nœuds CST bruts pour la
  conversion AST → modèle et le formatage), `chrono`, `rust_decimal`,
  `thiserror`. `toml` figure dans `Cargo.toml` comme dépendance déclarée
  mais n'est actuellement utilisée par aucun module de cette crate ; elle
  est réservée à un futur support de valeurs de métadonnées structurées.
- `parser.rs` et `printer.rs` passent tous deux par
  `pactole_syntax::parse_document`/`ParsedDocument::into_result` pour
  l'initialisation du parseur et la détection `ERROR`/`MISSING`, afin de
  ne pas dupliquer cette plomberie ; ils gardent ensuite chacun leur
  propre logique de parcours de nœuds tree-sitter (conversion vers modèle
  de domaine, respectivement formatage canonique).
- L'API stricte historique (`PactoleFileStorage`, `parser.rs`,
  `printer.rs`) reste inchangée par l'introduction de `pactole-syntax` et
  de `analysis.rs`/`loader.rs` : ces derniers sont strictement additifs.
- Aucun type `Span`/tree-sitter ne fuite dans `pactole-core` :
  `ParsedEntry`/`ParsedInclude`/`LoweringDiagnostic` vivent ici et ne font
  qu'envelopper `pactole_syntax::Span` autour de types `pactole-core`
  existants.

## Tests / validation

```sh
cargo test -p pactole-storage-fs
cargo clippy -p pactole-storage-fs --all-targets
```

Ajouter les tests unitaires de parsing/formatage dans cette crate (voir
`parser.rs`, `printer.rs`, `analysis.rs`, `loader.rs`).

## Voir aussi

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — flux strict vs
  tolérant, résolution multi-fichiers, roadmap (notamment les diagnostics
  métier positionnés, encore absents de cette crate).
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — spécification du langage
  `.pactole` que `parser.rs`/`printer.rs`/`analysis.rs` interprètent.
