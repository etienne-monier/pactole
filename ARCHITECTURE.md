# Architecture de Pactole

Ce document décrit l'architecture actuelle du dépôt Pactole : le découpage
en crates, leurs frontières, les flux de données ("strict" vs
"tolérant"), l'analyse multi-fichiers/`include`, le serveur LSP, les
invariants du langage, la configuration existante et une feuille de route
de ce qui reste à faire. Il complète (sans les dupliquer) `GRAMMAR.md`
(syntaxe `.pactole`) et `HELIX.md` (intégration éditeur via tree-sitter).

Pour le détail exhaustif module par module de chaque crate, voir aussi
`AGENTS.md` à la racine, qui sert de référence pour les agents IA
travaillant sur ce dépôt et que ce document résume/complète du point de
vue architecture.

## 1. Vue d'ensemble

Pactole est un espace de travail Cargo (`resolver = "3"`) composé de huit
crates :

```text
tree-sitter-pactole   (grammaire + parseur C, sans dépendance aux autres crates)
        │
        ▼
pactole-syntax        (analyse syntaxique tolérante : spans, diagnostics)
        │
        ▼
pactole-storage-fs ───┬──▶ pactole-core   (modèles de domaine, validation, reports)
        │             │        ▲
        │             └────────┘ (pactole-storage-fs dépend de pactole-core,
        │                         jamais l'inverse)
        ▼
   ┌────┴────┐
   ▼         ▼
pactole-cli  pactole-lsp
   │
   ▼
pactole-import        (adaptateur d'entrée : formats externes → domaine)

pactole-config        (schéma `pactole.toml` + résolution en couches ; crate
                       feuille consommée par pactole-storage-fs,
                       pactole-import, pactole-cli et pactole-lsp)
```

`pactole-core` ne dépend d'aucune autre crate du dépôt : c'est le domaine
pur (aucune E/S, aucun tree-sitter). `pactole-syntax` ne dépend que de
`tree-sitter-pactole` et ignore tout de `pactole-core`. `pactole-storage-fs`
est le seul point de jonction entre le monde syntaxique (tree-sitter /
`pactole-syntax`) et le monde domaine (`pactole-core`). `pactole-import` est
un adaptateur d'entrée symétrique : il dépend uniquement de `pactole-core` et
`pactole-config` (pas de tree-sitter, pas d'E/S) et traduit un format externe
(aujourd'hui OFX) en modèles de domaine. `pactole-config` est une crate
feuille : elle ne dépend d'aucune autre crate Pactole et centralise le format
`pactole.toml` et ses règles de fusion. `pactole-cli` et `pactole-lsp` sont
les deux points d'entrée binaires ; `pactole-cli` consomme en plus
`pactole-import` pour sa sous-commande d'import (voir §3, §6 et §2.7).

## 2. Frontières entre crates

### 2.1 `tree-sitter-pactole`

Grammaire tree-sitter du langage `.pactole` (`grammar.js`), parseur C
généré, bindings Rust (`bindings/rust/lib.rs` + `build.rs` qui compile le
parseur C via `cc`), et requêtes de coloration syntaxique
(`queries/highlights.scm`). Ne connaît rien du domaine métier Pactole :
c'est une définition de syntaxe et un parseur brut, réutilisable par tout
outil (éditeurs, autres langages, etc.). Voir `GRAMMAR.md` pour la
spécification complète et `HELIX.md` pour l'intégration éditeur.

### 2.2 `pactole-core`

Domaine pur : aucune E/S, aucune dépendance à tree-sitter ni aux autres
crates de ce dépôt (ne doit jamais dépendre de `pactole-storage-fs`,
`pactole-syntax` ou `tree-sitter`). Contient :

- les modèles (`models/`) : `Journal`, `Entry`, `Transaction`, `Posting`,
  `Amount`, `Balance`, `Open`, `Close`, `Commodity`, `Payee`, ainsi que des
  newtypes (`AccountName`, `CommodityName`, `MetadataKey`) ;
- `validation.rs` : invariants métier sur un `Journal` (ordre
  chronologique, cycle de vie des comptes, commodités déclarées, payees
  déclarés, équilibrage des transactions) ;
- `register.rs` : calcul du rapport "register" (solde courant par
  commodité, filtrage par compte/date/payee/narration/statut/tag) ;
- `traits.rs` : abstraction de stockage (`ReadableStorage`) ;
- `errors.rs` : `ModelError`, `ValidationError`.

L'arithmétique utilise systématiquement `rust_decimal::Decimal`, jamais de
flottants.

### 2.3 `pactole-syntax`

Analyse syntaxique **tolérante** : le parsing ne renvoie jamais d'échec
irrécupérable, même sur un texte invalide. Sans E/S et sans dépendance à
`pactole-core`. Fournit :

- `span.rs` : `Point` (offset en octets + ligne/colonne 0-indexées) et
  `Span` (paire de `Point` en intervalle semi-ouvert, avec
  `byte_range()`) ;
- `diagnostics.rs` : `SyntaxDiagnostic` (un `DiagnosticKind`, un `Span`,
  un message) et `DiagnosticKind` (`Error` pour les nœuds `ERROR`
  tree-sitter, `Missing` pour les nœuds "missing") ;
- `document.rs` : `ParsedDocument` (source + `Tree` tree-sitter +
  diagnostics collectés), `parse_document`/`analyze` (point d'entrée du
  parsing tolérant, `analyze` étant un alias de `parse_document`), et
  `collect_error_nodes` (parcourt un `Tree` et collecte les nœuds
  `ERROR`/`MISSING`). `ParsedDocument::into_result` offre une conversion
  stricte en `Result` pour les consommateurs qui ne veulent avancer que
  sur un document syntaxiquement propre.

Ce crate est le socle syntaxique partagé, pensé pour être réutilisé par de
futurs outils (comme `pactole-lsp`) sans dupliquer la plomberie
tree-sitter.

### 2.4 `pactole-storage-fs`

Seul point de jonction entre syntaxe (tree-sitter / `pactole-syntax`) et
domaine (`pactole-core`). Implémente `ReadableStorage` pour le système de
fichiers local (`PactoleFileStorage`). Contient deux flux distincts,
détaillés au §3 : le flux **strict** (`parser.rs`/`printer.rs`, API
historique inchangée) et le flux **tolérant/positionné**
(`analysis.rs`/`loader.rs`, ajout additif pour l'outillage).

- `parser.rs` : convertit le CST tree-sitter en modèles `pactole-core`, et
  résout récursivement les directives `include`.
- `printer.rs` : formate un AST/du code source dans la mise en page
  canonique `.pactole`. Expose aussi `format_transactions(&[Transaction])`,
  qui sérialise une liste de transactions du domaine en texte `.pactole`
  canonique (utilisé par `pactole import-ofx` ; c'est l'inverse du parseur
  pour ce sous-ensemble de directives).
- `errors.rs` : `PactoleFsStorageError`.
- `analysis.rs` : API positionnée par directive (`analyze_file`,
  `ParsedFile`, `ParsedEntry`, `ParsedInclude`, `LoweringDiagnostic`) et
  API multi-fichiers (`analyze_file_with_loader`, `AnalyzedProject`,
  `AnalyzedFile`, `IncludeIssue`).
- `loader.rs` : abstraction `SourceLoader` (+ `FsSourceLoader`,
  `InMemorySourceLoader`, `SourceLoadError`) sur "d'où vient le texte
  source", indépendante du système de fichiers réel.

`analyze_file_with_loader_with_options(entry_path, loader, &ValidationOptions)`
est la variante configurable : elle reclasse le seul contrôle métier
configurable (`PayeeNotDeclared`) en avertissement quand
`[validation] payee = "warn"`, tandis que tous les autres problèmes métier
restent des erreurs. `validated_journal()` réussit alors tant qu'aucun
diagnostic de sévérité « erreur » ne subsiste : les avertissements seuls ne
bloquent pas. La forme `analyze_file_with_loader` utilise
`ValidationOptions::default()` (tout en erreur), donc le comportement des
appelants existants est inchangé. Les types de configuration
(`ValidationOptions`, `FormatOptions`) proviennent de `pactole-config` ;
`pactole-storage-fs` réexporte `FormatOptions` pour garder son API de
formatage autonome.

`parser.rs` et `printer.rs` réutilisent `pactole_syntax::parse_document` /
`ParsedDocument::into_result`, pour ne pas dupliquer l'initialisation du
parseur tree-sitter ni la détection `ERROR`/`MISSING`. Le formateur
canonique reste dans cette crate (il n'est ni déplacé ni dupliqué dans
`pactole-syntax`). L'API stricte historique (`PactoleFileStorage`,
`parser.rs`, `printer.rs`) est inchangée par l'introduction de
`pactole-syntax`/`analysis.rs`.

### 2.5 `pactole-lsp`

Serveur LSP minimal en stdio (`lsp-server` + `lsp-types`), binaire
`pactole-lsp`. Ne consomme `pactole-storage-fs`/`pactole-syntax` que via
leur API publique existante (`analyze_file`, `analyze_file_with_loader`,
`analyze_file_with_loader_with_options`, `FsSourceLoader`, `SourceLoader`,
`SourceLoadError`, `ParsedFile`, `AnalyzedProject`, `IncludeIssue`,
`SyntaxDiagnostic`, `Span`/`Point`) et utilise `pactole-config` pour la
configuration en couches : il n'ajoute ni ne modifie rien dans ces crates.
Détaillé au §5.

### 2.6 `pactole-cli`

Binaire `pactole` (via `clap` derive), consommant `pactole-core` et
`pactole-storage-fs` par leur API stricte
(`PactoleFileStorage`/`validate_journal`/`register`/`format`),
`pactole-import` pour l'import de relevés externes et `pactole-config` pour
résoudre `pactole.toml`. Détaillé au §6.

### 2.7 `pactole-import`

Adaptateur d'entrée (edge adapter), symétrique de `pactole-storage-fs` :
il traduit un format externe vers le domaine pur. Il ne dépend que de
`pactole-core` et `pactole-config` (plus `chrono`/`rust_decimal`), effectue
**aucune E/S** (le texte source lui est fourni sous forme de `&str`) et ne
connaît ni tree-sitter, ni `pactole-storage-fs`, ni le format de fichier de
configuration. Ce positionnement le rend réutilisable par n'importe quel
front-end : la CLI aujourd'hui, un futur `pactole-storage-db` (qui y
trouvera de quoi alimenter une base) et le LSP demain. Contient :

- `record.rs` : `ImportRecord`, représentation intermédiaire neutre vis-à-vis
  du format (date, montant signé, devise, payee, mémo, type, FITID...).
- `ofx.rs` : parseur OFX (1.x SGML et 2.x XML, via la crate `ofx-rs`) qui
  produit des `ImportRecord`s à partir d'un texte OFX.
- `rules.rs` : moteur de règles partagé qui abaisse un `ImportRecord` en
  `pactole_core::Transaction` équilibrée (règle : premier motif gagnant,
  repli sur le compte par défaut).
- `error.rs` : `ImportError`.

`ImportConfig`, `Rule` et `Direction` sont désormais **définis dans
`pactole-config`** (section `[import]` de `pactole.toml`) et simplement
réexportés ici pour la commodité des appelants.

Le point d'entrée public, `import_ofx(source, &config, known_fitids)`,
applique l'idempotence : toute transaction source dont le `FITID` figure
déjà dans `known_fitids` est comptée comme « sautée » et omise. Chaque
transaction produite porte son `FITID` dans une métadonnée `fitid`, ce qui
permet de reconstituer `known_fitids` lors d'une ré-importation. Le flux est
volontairement découpé en deux étapes (parseur de format → `ImportRecord`,
puis moteur de règles → `Transaction`) afin que de futurs importeurs
(CSV, QIF...) réutilisent le même moteur de règles sans le dupliquer.

La sérialisation `.pactole` des transactions produites reste dans
`pactole-storage-fs` (`format_transactions`, voir §2.4), pas dans cette
crate : `pactole-import` ignore tout du format de sortie textuel.

### 2.8 `pactole-config`

Crate feuille : ne dépend d'aucune autre crate Pactole (uniquement `serde`,
`toml` et `thiserror`). Elle centralise le schéma complet de `pactole.toml`
(`[validation]`, `[format]`, `[import]`) et sa résolution en couches, que
tous les front-ends partagent. Voir §8 pour le détail du format et des
priorités.

## 3. Flux strict vs flux tolérant

Le dépôt expose deux façons bien distinctes de traiter un fichier
`.pactole`, choisies selon le besoin du consommateur :

### 3.1 Flux strict (`pactole-cli`, historique)

`PactoleFileStorage` (dans `pactole-storage-fs`) : lit un fichier, le
parse via `parser::parse`, qui échoue **entièrement** (renvoie un
`Result::Err`) dès la première erreur de syntaxe ou de construction du
modèle, y compris dans un fichier inclus. Produit, en cas de succès, un
`pactole_core::Journal` unique et aplati (tous les fichiers inclus fondus
en une seule structure), que l'on peut ensuite passer à
`pactole_core::validate_journal` (invariants métier) ou
`pactole_core::register` (rapport). C'est le flux utilisé par
`pactole check`, `pactole parse`, `pactole register`. Le formatage
canonique (`pactole_storage_fs::format`, utilisé par `pactole fmt`) suit
la même logique "tout ou rien".

### 3.2 Flux tolérant/positionné (`pactole-lsp`, outillage)

`analyze_file`/`analyze_file_with_loader` (dans
`pactole-storage-fs::analysis`) : ne renvoient jamais d'échec global,
même sur un fichier partiellement ou totalement invalide.

- `analyze_file(source: &str) -> ParsedFile` : analyse un unique fichier
  (pas de résolution d'`include`). Chaque directive de haut niveau est
  abaissée (« lowered ») indépendamment des autres : en cas de succès elle
  devient un `ParsedEntry { span, entry }` (réutilisant
  `pactole_syntax::Span` et `pactole_core::Entry`) ; en cas d'échec, elle
  devient un `LoweringDiagnostic { span, message }` et l'analyse continue
  avec la directive suivante au lieu d'abandonner tout le fichier. Les
  directives `include` sont seulement *détectées* comme
  `ParsedInclude { span, path }` avec leur chemin brut, non résolu :
  `analysis.rs` ne lit ni ne parse le fichier cible. `ParsedFile`
  réutilise la même logique d'abaissement AST que `parser.rs` (via
  `AstBuilder::build_directive_outcome`, privé au crate) : il n'y a pas de
  second parseur. `ParsedFile::syntax_diagnostics()` expose en plus les
  `SyntaxDiagnostic`s bruts de `pactole-syntax` (nœuds `ERROR`/`MISSING`),
  pour distinguer les problèmes syntaxiques des problèmes d'abaissement.
- `analyze_file_with_loader(entry_path, loader) -> Result<AnalyzedProject, ...>`
  : suit récursivement les `include` via un `&dyn SourceLoader`, en
  réutilisant `analyze_file` par fichier. Contrairement au flux strict, il
  ne fusionne jamais les fichiers en un seul `Journal` : chaque fichier
  garde son propre `ParsedFile` (entrées et diagnostics positionnés) dans
  un `AnalyzedFile { path, file }`, ce qui préserve la provenance
  par-fichier nécessaire à un outil qui doit rapporter un diagnostic
  contre le fichier dont il provient. Voir §4 pour le détail de la
  résolution multi-fichiers. Seul un échec de chargement du fichier
  d'entrée lui-même est renvoyé en `Err` (rien à analyser dans ce cas) ;
  tout problème sur un fichier inclus devient un `IncludeIssue`, jamais un
  abandon global.

Ces deux flux ne s'excluent pas : `pactole-storage-fs` les expose tous les
deux, chacun consommé par le binaire pour lequel il est adapté
(`pactole-cli` pour le strict, `pactole-lsp` pour le tolérant).

## 4. Analyse multi-fichiers et résolution des `include`

`analyze_file_with_loader` s'appuie sur `loader.rs` :

- `SourceLoader` est un petit trait (`fn load(&self, path: &Path) ->
  Result<String, SourceLoadError>`) qui ne fait que charger le texte
  source brut d'un chemin déjà résolu ; il ne connaît rien à la syntaxe
  `.pactole`.
- `FsSourceLoader` lit de vrais fichiers (c'est ce qu'utilisent
  implicitement `parser.rs`/`PactoleFileStorage`, sans changement).
- `InMemorySourceLoader` sert du contenu depuis une table en mémoire
  `chemin -> String`, ce qui préfigure un futur chargeur `pactole-lsp`
  adossé aux buffers ouverts (potentiellement non sauvegardés) de
  l'éditeur.

Propriétés de la résolution multi-fichiers :

- Elle ne fait **jamais échouer le contenu** des fichiers inclus : un
  `include` relatif résolu sans répertoire de base joignable, une cible
  que le loader ne peut fournir, ou un cycle d'inclusion, sont tous
  rapportés comme des `IncludeIssue { path, include, message }` sur
  l'`AnalyzedProject`, plutôt que d'interrompre toute l'analyse.
- Un include "en losange" (le même fichier atteint par deux chemins
  différents) n'est visité qu'une seule fois.
- Un auto-include ou un cycle mutuel est détecté via une pile de chemins
  en cours de visite, et rapporté comme un `IncludeIssue`, jamais abaissé.
- Volontairement minimal : pas de cache, pas de ré-analyse incrémentale,
  et aucun suivi de "quels fichiers chargés une analyse donnée
  dépend-elle" pour invalider un résultat. Un futur `pactole-lsp` est
  censé construire cela par-dessus ce contrat plutôt que de faire grossir
  prématurément cette crate.

## 5. LSP actuel (`pactole-lsp`)

Serveur stdio minimal (`lsp-server`/`lsp-types`), synchronisation
**complète** des documents (`TextDocumentSyncKind::FULL`) : chaque
`didChange` transporte le texte entier, donc aucune conversion
position-LSP → offset-octet n'est nécessaire pour appliquer les édits.
C'est un compromis simplicité/robustesse assumé par rapport à une
synchronisation incrémentale, acceptable pour des fichiers `.pactole`
typiquement petits et édités à la main.

Modules :

- `documents.rs` : `Documents` (table en mémoire URI ouverte → texte
  courant) et `DocumentsSourceLoader`, un `SourceLoader` qui sert le
  buffer d'un document ouvert (potentiellement non sauvegardé) quand il
  en existe un pour un chemin donné, avec repli sur `FsSourceLoader`
  sinon. Cela fait primer les modifications non sauvegardées d'un fichier
  *inclus* en cours d'édition sur son contenu sauvegardé sur disque lors
  de la résolution des `include`.
- `conversion.rs` : `span_to_range`/`point_to_position`, convertissant les
  `Span`/`Point` en octets de `pactole-syntax` vers les `Range`/`Position`
  en UTF-16 de LSP, en rebalayant la ligne source concernée (un offset en
  octets seul ne détermine pas un offset en caractères UTF-16 dès que la
  source contient des caractères non-ASCII).
- `diagnostics.rs` : `diagnostics_for_file` (diagnostics syntaxe +
  abaissement pour un `ParsedFile`) et `project_diagnostics` (diagnostics
  par fichier, plus les `IncludeIssue` rattachées au fichier contenant
  l'`include` fautif, pour un `AnalyzedProject`).
- `config.rs` : `Config`, résolue une seule fois à l'`initialize` à partir
  des `initializationOptions` et des fichiers `pactole.toml` en couches.
  Supporte `journal_file` (chemin vers le fichier `.pactole` racine, résolu
  contre la racine du workspace — depuis `workspace_folders`, puis
  `rootUri`, puis `rootPath` — s'il est donné relatif ; si aucune racine de
  workspace n'est connue du tout, résolu contre le répertoire de travail
  courant du process serveur plutôt que laissé relatif tel quel, pour ne
  pas échouer silencieusement à se résoudre plus tard et faire disparaître
  silencieusement tous les diagnostics de tout le graphe d'`include`), et
  `validation` (une chaîne `"warn"`/`"error"`, ou une table
  `{ "payee": ... }`). La validation suit la même stratification que la CLI :
  le `pactole.toml` global (via `pactole_config::config_dir`), puis le
  `pactole.toml` du projet (à côté de `journal_file`, ou à la racine du
  workspace quand aucun `journal_file` n'est configuré), puis
  `initializationOptions`. Un fichier de configuration illisible ou
  invalide, ainsi que le niveau `off` pas encore supporté, sont journalisés
  et ignorés plutôt que de faire échouer `initialize`.
- `server.rs` : la `Connection` stdio, la boucle principale, et les
  handlers de notifications. Publie `textDocument/publishDiagnostics` :
  - quand `journal_file` est configuré et se charge avec succès, les
    diagnostics sont calculés pour **chaque fichier atteignable via
    `include`** avec `analyze_file_with_loader` et `DocumentsSourceLoader`
    (buffers ouverts d'abord, repli sur le disque), et publiés par
    fichier (indexés par le chemin/URI propre à chaque fichier) ;
  - sinon (pas de `journal_file`, ou son chargement échoue), le document
    courant seul est analysé de façon autonome via `analyze_file` (ses
    `include` ne sont *pas* suivis dans ce chemin de repli) ;
  - la boucle principale garde trace de l'ensemble des URI publiées avec
    des diagnostics par le dernier appel `publish_for` ; toute URI qui
    n'est plus couverte par le dernier appel (par exemple un `include` a
    été supprimé, faisant sortir un fichier du projet) reçoit une liste de
    diagnostics vide, pour que les diagnostics ne restent jamais périmés
    une fois qu'un fichier sort de l'ensemble analysé.

Explicitement hors du périmètre de cette première version : navigation,
formatage via LSP (utiliser `pactole fmt` pour le formatage) et
synchronisation incrémentale. La complétion, le survol (hover) et les
actions de code sont, eux, supportés. Les requêtes non gérées reçoivent une
erreur `MethodNotFound` plutôt que d'être silencieusement ignorées.

Journalisation (`log`/`env_logger`) : silencieuse par défaut, activable à la
demande via `RUST_LOG` (ex. `RUST_LOG=pactole_lsp=debug`) ; écrite
uniquement sur stderr, jamais sur stdout (réservé au transport du
protocole LSP) ; ne contient jamais le contenu des documents ouverts, pour
rester sans risque à activer sur des journaux potentiellement sensibles.

## 6. `pactole-cli`

Binaire `pactole` (`clap` derive). Drapeaux **racine** (avant la
sous-commande) :

- `--config <path>` : utilise ce `pactole.toml` comme unique couche et
  court-circuite la découverte global/projet ;
- `--validation <warn|error>` : surcharge le niveau `[validation] payee`.

Sous-commandes actuelles :

- `pactole parse <file>` : parse le fichier et affiche (`Debug`) les
  entrées du journal résultant. Surtout utile pour déboguer la
  grammaire/le parseur.
- `pactole fmt <file> [--write]` : reformate un fichier `.pactole` dans sa
  forme canonique (stdout par défaut, `--write` pour écrire sur place ;
  `-` accepté comme nom de fichier pour lire depuis stdin, toujours écrit
  sur stdout dans ce cas), en utilisant les options `[format]` résolues.
- `pactole completions <shell>` : génère un script de complétion.
- `pactole check <file> [--strict]` : parsing syntaxique **puis** validation
  métier (`pactole_core::validate_journal`), avec les niveaux `[validation]`
  résolus ; `--strict` transforme les avertissements en échec.
- `pactole register <file> [filtres]` : rapport "register" avec solde
  courant, filtrable par `--account` (avec sous-comptes), `--from`/`--to`,
  `--payee`, `--narration`, `--tag`, `--status`.
- `pactole init [ledger] [--global]` : crée un journal de départ et, s'il
  manque, un `pactole.toml` à côté du journal (ou dans le répertoire de
  configuration utilisateur avec `--global`). Les fichiers existants ne
  sont jamais écrasés.
- `pactole import-ofx <file> [--into <path>] [--account <compte>] [--write]` :
  importe un relevé OFX/QFX (SGML 1.x ou XML 2.x) et produit des directives
  de transaction `.pactole` canoniques (via
  `pactole_import::import_ofx` puis
  `pactole_storage_fs::format_transactions`). Le compte banque, la table de
  correspondance `[import.accounts]` (identifiant de compte du relevé →
  compte Pactole), le compte de contrepartie par défaut et les règles
  `[[import.rules]]` proviennent de la section `[import]` du `pactole.toml`
  résolu (voir §8) ; la commande échoue avec un message clair quand aucune
  section `[import]` n'est configurée. `--account` surcharge le compte
  bancaire configuré (et la table `[import.accounts]`) pour la seule
  exécution. Sans `--into`, les transactions sont imprimées sur stdout. Avec
  `--into <fichier>`, les `FITID` déjà présents dans ce fichier sont sautés
  (idempotence), et avec `--write` les transactions générées y sont ajoutées
  (append) au lieu d'être imprimées. Un compte de relevé absent de la table
  `[import.accounts]` est reposté sur `bank_account` avec un avertissement
  sur stderr.

Tests d'intégration bout en bout dans
`crates/pactole-cli/tests/cli.rs`.

## 7. Invariants du langage

Voir `GRAMMAR.md` pour la spécification complète. Rappel des invariants
clés portés par `pactole-core::validation` :

- Les directives démarrent en colonne 0 sur une ligne d'en-tête (`open`,
  `close`, `commodity`, `payee`, `balance`, transaction, `include`).
- Les lignes de métadonnées (`clé: valeur`) suivent immédiatement l'en-tête
  d'une directive, avant toute posting ; une métadonnée de posting est
  indentée sous cette posting.
- Toute transaction requiert un payee obligatoire ; en validation
  (`check`), ce payee doit être déclaré via une directive `payee`
  (contrôle configurable : `[validation] payee = "warn"` le rétrograde en
  avertissement).
- Les postings d'une transaction doivent s'équilibrer à zéro par
  commodité ; au plus une posting peut omettre son montant (montant
  déduit).
- Un compte doit être ouvert avant d'être utilisé (transaction, assertion
  de solde, fermeture) ; un compte fermé ne peut plus être utilisé après
  sa date de fermeture.

## 8. Configuration actuelle

Un seul fichier **`pactole.toml`**, résolu par `pactole-config` (§2.8) et
partagé par la CLI et le LSP. Les couches vont de la moins prioritaire à la
plus prioritaire :

1. le fichier global `~/.config/pactole/pactole.toml` (ou
   `$XDG_CONFIG_HOME/pactole/pactole.toml` si `XDG_CONFIG_HOME` est défini ;
   `%APPDATA%\pactole\` sous Windows) ;
2. le fichier projet `pactole.toml` — à côté du journal traité côté CLI,
   à côté de `journal_file` (ou à la racine du workspace si aucun) côté LSP ;
3. les surcharges en mémoire — drapeaux racine `--config`/`--validation` de
   la CLI, `initializationOptions` du LSP. `--config <path>` remplace la
   découverte par ce fichier unique (erreur si illisible).

La fusion se fait **par clé** (une clé absente d'une couche supérieure
conserve la valeur de la couche inférieure) ; `[import.accounts]` fusionne
par clé, `[[import.rules]]` est remplacé en bloc par la couche la plus
prioritaire qui les définit, et les clés inconnues sont rejetées par
section (`deny_unknown_fields`), donc les fautes de frappe échouent
bruyamment.

Schéma :

- `[validation] payee` : `"off"`, `"warn"` ou `"error"` (défaut `error`,
  comportement historique conservé). Ce niveau pilote les deux contrôles
  portant sur le payee : une transaction doit porter un payee
  (`PayeeMissing` sinon) et ce payee doit avoir été déclaré
  (`PayeeNotDeclared`). `warn` rétrograde les deux en avertissement, `off`
  les supprime tous les deux ; les autres contrôles métier restent des
  erreurs quel que soit le niveau.
- `[format] posting_width` : colonne d'alignement des montants (défaut 50).
- `[import]` : `bank_account`, `currency`,
  `default_counter_account`, `[import.accounts]`, `[[import.rules]]` — les
  réglages de `pactole import-ofx`.

Chaque commande distingue avertissements et erreurs : `pactole check
--strict` transforme les avertissements en échec, et `--validation` permet
de surcharger le niveau sans éditer de fichier.

- **LSP** (`pactole-lsp`) : en plus des couches ci-dessus, `journal_file`
  reste une clé d'`initializationOptions` propre, et `validation` peut y être
  surchargé sous forme de chaîne (`"warn"`) ou de table
  (`{ "payee": "warn" }`). Un fichier de configuration illisible ou invalide
  est journalisé et ignoré, jamais bloquant (voir §5).
- **Grammaire tree-sitter** : `tree-sitter.json` déclare les métadonnées
  du langage (`scope: source.pactole`, extension `.pactole`) pour les
  outils de l'écosystème tree-sitter (voir aussi `HELIX.md`).

## 9. Décisions et limites connues

- Synchronisation LSP **complète** plutôt qu'incrémentale : choix
  délibéré de simplicité/robustesse, à revisiter si l'édition de très
  gros journaux devient un problème.
- Le formateur canonique reste dans `pactole-storage-fs` (pas de
  duplication ni de déplacement vers `pactole-syntax`), pour ne pas
  élargir le périmètre de `pactole-syntax` au-delà du strictement
  syntaxique.
- `analyze_file_with_loader` ne détecte que les includes déjà résolus par
  le loader fourni : aucune racine `include` implicite (variables
  d'environnement, chemins de recherche) n'est gérée au-delà de la
  résolution relative au fichier courant.
- Pas de cache ni de ré-analyse incrémentale dans
  `pactole-storage-fs::analysis`/`loader` : chaque appel refait tout le
  travail depuis zéro. Un futur `pactole-lsp` devra construire toute
  logique d'invalidation par-dessus, pas dans cette crate.
- `pactole-lsp` lit désormais la configuration stratifiée `pactole.toml`
  (voir §8) comme la CLI, au même titre que ses `initializationOptions`.
- `pactole-import` ne fait aucune E/S et ne connaît ni tree-sitter ni le
  format de fichier de configuration : c'est `pactole-config` qui lit le
  TOML et la CLI qui fournit un `ImportConfig` à `import_ofx`. Cette
  frontière garde l'importeur réutilisable par un futur
  `pactole-storage-db` (alimenter une base au lieu d'écrire un fichier)
  sans le coupler au système de fichiers.
- `format_transactions` (sérialisation domaine → texte `.pactole`) est un
  sérialiseur de sortie best-effort : la grammaire n'autorisant ni `"`
  ni saut de ligne dans une chaîne, ces caractères y sont remplacés par
  une espace. Les valeurs de métadonnées qui ressemblent à une date ou à
  un nombre au sens de la grammaire sont émises sans guillemets, les
  autres entre guillemets.
- Le MSRV du workspace est **1.88** (et non plus 1.85) : `ofx-rs` dépend
  de `time 0.3.47+`, qui exige Rust 1.88. Le dépôt ne compile donc plus
  sur 1.85 tant que `pactole-import` utilise `ofx-rs`. Ce choix est
  assumé (plutôt qu'un parseur OFX maison) ; il faudra le revoir si un
  retour à 1.85 devient nécessaire.
- Aucune crate de stockage base de données n'existe encore (voir §10).

## 10. Roadmap

Ce qui reste explicitement à faire, par ordre approximatif de valeur/
complexité croissante :

1. **Autres importeurs** (CSV, QIF...) : `pactole-import` est structuré
   pour cela (parseur de format → `ImportRecord`, puis moteur de règles
   partagé → `Transaction`), mais seul OFX est implémenté. Ajouter un
   importeur consiste à écrire un nouveau module produisant des
   `ImportRecord`s et à exposer un point d'entrée analogue à
   `import_ofx`.
2. **Navigation LSP** ("go to definition"/"find references") : la
   complétion, le survol et les actions de code existent déjà, mais la
   navigation est encore absente.
3. **Formatage via LSP** (`textDocument/formatting`) : aujourd'hui,
   formater nécessite la CLI (`pactole fmt`) ; `pactole-lsp` ne l'expose
   pas.
4. **Synchronisation incrémentale** (`TextDocumentSyncKind::INCREMENTAL`)
   si la synchronisation complète actuelle s'avère limitante sur de gros
   journaux.
5. **`pactole-storage-db`** (nom provisoire) : future crate de stockage
   adossée à une base de données, implémentant
   `pactole_core::traits::ReadableStorage` (et possiblement un pendant
   accessible en écriture) à la place du système de fichiers, en miroir
   du rôle de `pactole-storage-fs` mais sans les préoccupations
   tree-sitter/parsing de fichiers. Référencée ici pour la planification
   architecturale uniquement ; ne pas créer de crate stub sans demande
   explicite.

## 11. Validation et tests

Voir `AGENTS.md` (§4) pour les commandes de référence
(`cargo test --workspace`, `cargo clippy --workspace --all-targets`,
`cargo fmt --check`, `cargo doc --workspace --no-deps`). Chaque README de
crate détaille les commandes de test ciblées pour cette crate.
