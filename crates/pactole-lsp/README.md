# pactole-lsp

Serveur [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
minimal en stdio pour les fichiers `.pactole`.

## Rôle

Binaire `pactole-lsp`, construit sur `lsp-server` + `lsp-types`. Gère le
cycle de vie LSP (`initialize`/`shutdown`/`exit`) et la synchronisation de
documents `textDocument/didOpen`/`didChange`/`didClose` en
synchronisation **complète** (`TextDocumentSyncKind::FULL`) : chaque
`didChange` transporte le texte entier du document, donc aucune
conversion position LSP → offset octet n'est nécessaire pour appliquer un
édit. Choix délibéré de simplicité/robustesse par rapport à une
synchronisation incrémentale, acceptable pour des fichiers `.pactole`
typiquement petits et édités à la main.

Ne consomme `pactole-storage-fs`/`pactole-syntax` que via leur API
publique existante (`analyze_file`, `analyze_file_with_loader`,
`FsSourceLoader`, `SourceLoader`, `SourceLoadError`, `ParsedFile`,
`AnalyzedProject`, `IncludeIssue`, `SyntaxDiagnostic`, `Span`/`Point`) :
n'ajoute ni ne modifie rien dans ces crates.

## Fonctionnalités actuelles

Diagnostics (`textDocument/publishDiagnostics`) uniquement, via :

- `documents.rs` : `Documents` (table en mémoire URI ouverte → texte
  courant) et `DocumentsSourceLoader`, un
  `pactole_storage_fs::SourceLoader` qui sert le buffer d'un document
  ouvert (potentiellement non sauvegardé) quand il en existe un pour un
  chemin donné, avec repli sur `FsSourceLoader` sinon.
- `conversion.rs` : `span_to_range`/`point_to_position`, convertissant les
  `Span`/`Point` en octets de `pactole-syntax` vers les `Range`/`Position`
  en UTF-16 de LSP, en rebalayant la ligne source concernée.
- `diagnostics.rs` : `diagnostics_for_file` (diagnostics syntaxe +
  abaissement pour un `ParsedFile`) et `project_diagnostics` (diagnostics
  par fichier, plus les `IncludeIssue` rattachées au fichier contenant
  l'`include` fautif, pour un `AnalyzedProject`).
- `config.rs` : `Config`, résolue une seule fois à partir des
  `initializationOptions` reçues à l'`initialize`. Une seule clé
  optionnelle supportée : `journal_file` (chemin du fichier `.pactole`
  racine, résolu contre la racine du workspace, ou contre le répertoire
  de travail courant du serveur si aucune racine n'est connue). **Ne lit
  pas `~/.config/pactole`** : limitation explicite et documentée de cette
  première version.
- `server.rs` : la `Connection` stdio, la boucle principale, et les
  handlers de notifications. Quand `journal_file` est configuré et se
  charge, les diagnostics sont calculés pour tout fichier atteignable via
  `include` (via `analyze_file_with_loader`) et publiés par fichier ;
  sinon le document courant seul est analysé de façon autonome via
  `analyze_file` (ses `include` ne sont alors pas suivis). Les URI qui
  sortent de l'ensemble analysé (par exemple un `include` supprimé)
  reçoivent une liste de diagnostics vide pour ne jamais rester périmées.

## Explicitement hors périmètre (première version)

Complétion, hover, navigation, formatage via LSP (utiliser `pactole fmt`),
synchronisation incrémentale, lecture de `~/.config/pactole`. Toute
requête non gérée reçoit une erreur `MethodNotFound` plutôt que d'être
silencieusement ignorée. Voir la roadmap dans
[`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §10 pour l'état
d'avancement prévu de ces points.

## Configuration côté client

Passer `journal_file` dans `initializationOptions` de la requête LSP
`initialize`, par exemple :

```json
{ "journal_file": "main.pactole" }
```

## Dépendances / frontières

- `lsp-server`, `lsp-types` (protocole LSP), `serde`/`serde_json`
  (désérialisation JSON), `pactole-storage-fs`, `pactole-syntax`.
- `log` + `env_logger` pour les traces de debug (voir ci-dessous).
- Pas de dépendance à `thiserror` : aucun type d'erreur propre, seule la
  propagation via `Box<dyn Error + Sync + Send>` dans `server.rs`.

## Traces de debug

Le serveur journalise via [`log`](https://docs.rs/log) +
[`env_logger`](https://docs.rs/env_logger), **toujours sur `stderr`** (jamais
sur `stdout`, qui sert au protocole LSP lui-même) : une écriture parasite sur
`stdout` corromprait le flux de messages. Sans `RUST_LOG`, aucune trace n'est
émise.

Activer les traces en définissant `RUST_LOG` avant de lancer `pactole-lsp`,
par exemple :

```sh
RUST_LOG=pactole_lsp=debug pactole-lsp
```

Niveaux utilisés :

- `info` : cycle de vie du serveur (initialisation avec la racine du
  workspace et le `journal_file` résolu, arrêt).
- `debug` : notifications `didOpen`/`didChange`/`didClose` (URI et taille du
  texte, jamais son contenu), résultat de chaque analyse (nombre de fichiers
  et de diagnostics publiés en mode multi-fichiers, nombre de diagnostics en
  mode autonome).
- `warn` : repli de l'analyse multi-fichiers vers l'analyse autonome du
  document courant (`journal_file` configuré mais illisible), et effacement
  de tous les diagnostics publiés faute d'analyse disponible.
- `trace` : origine de chaque fichier chargé pendant la résolution des
  `include` (tampon ouvert en mémoire ou système de fichiers), via
  `DocumentsSourceLoader`.

Le contenu des documents n'est jamais journalisé, seules des métadonnées
(URI, longueurs, chemins) le sont.

## Tests / validation

```sh
cargo test -p pactole-lsp
cargo clippy -p pactole-lsp --all-targets
```

Les tests unitaires couvrent notamment `config.rs` (résolution de
`journal_file`) et `server.rs` (calcul des URI à effacer entre deux
publications de diagnostics).

## Voir aussi

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §5 — description
  complète du LSP actuel, ses limites, et la roadmap (config globale,
  diagnostics métier positionnés, complétion/hover/navigation, formatage
  LSP, synchronisation incrémentale).
