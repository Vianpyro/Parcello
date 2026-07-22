# Audit — dette H1 : miroir Dart manuel du protocole (drift silencieux)

Statut : **document de travail, non validé**. Rien n'a été implémenté. Objectif :
poser un état des lieux complet et argumenter un choix de stratégie avant toute
modification de code. Rédigé en français pour coller à la demande ; à traduire
en anglais avant fusion dans `docs/` si une stratégie est retenue (le reste du
dossier est en anglais).

Périmètre inspecté : `crates/protocol/src/lib.rs`, `crates/engine/src/{command,event,error,content,view,state}.rs`,
`crates/mods/src/{loader,manifest}.rs`, `clients/flutter/lib/{protocol,session,director,motion}.dart`
et tous les sites d'appel de `sendCmd(...)` dans `clients/flutter/lib/ui/**`,
`crates/cli/src/*.rs` (comme point de comparaison), `.github/workflows/{ci,flutter}.yml`.

---

## 1. Résumé exécutif

Le protocole réseau (`ClientMessage`/`ServerMessage`/`CommandKind`/`Event`/
`CommandError`/toutes les structures de vue) est défini **une seule fois en
Rust**, dans `crates/protocol` et `crates/engine`. Le client CLI Rust
(`crates/cli`) importe ces types directement — zéro duplication, zéro risque de
drift, le compilateur casse toute divergence.

Le client Flutter, lui, ne peut pas importer du Rust : il **réécrit à la main**
la totalité de ces formes en Dart. Ce n'est pas une seule duplication, mais un
faisceau d'au moins **six mécanismes de duplication indépendants** (détaillés
section 2), plus un **trou concret dans la CI** qui transforme cette duplication
en drift *réellement* silencieux : un changement du protocole côté Rust qui ne
touche pas `clients/flutter/**` ne déclenche aujourd'hui **aucun job Flutter**
(`flutter.yml` est filtré sur ce chemin ; `ci.yml` l'ignore explicitement en
retour). Une variante `Event` renommée, un champ ajouté sur `ClientView`, une
constante de timing changée : tout cela peut être mergé sur `main`, vert de bout
en bout, sans qu'aucun test ne l'ait vu passer sous les yeux du client Flutter.

La duplication n'est donc pas seulement un problème de confort de
maintenance : c'est un problème de **détection**. Le cœur du problème n'est pas
« on retape le même code deux fois », c'est « rien ne dit jamais que les deux
copies ont divergé ».

---

## 2. Inventaire complet des duplications

### 2.1 Messages client → serveur (`ClientMessage`, 17 variantes)

Rust (`crates/protocol/src/lib.rs:62-150`) définit un enum externally-tagged
(`#[serde(tag = "type", rename_all = "snake_case")]`) avec 17 variantes :
`Create`, `Join`, `Spectate`, `AddBot`, `RemoveBot`, `Configure`, `Start`,
`PlayAgain`, `Leave`, `Cmd`, `Feedback`, `AnimationDone`, `ListMods`,
`QueueRanked`, `CancelQueue`, `GetRating`, `Ping`.

Côté Dart, **il n'existe aucun type miroir**. `GameSession` (session.dart)
construit chaque message comme un `Map<String, dynamic>` littéral encodé à la
volée :

```dart
_ws?.sink.add(jsonEncode({'type': 'create', 'auth': _auth(''), if (mods.isNotEmpty) 'mods': mods}));
_ws?.sink.add(jsonEncode({'type': 'join', 'code': c, 'auth': _auth(c)}));
_ws?.sink.add(jsonEncode({'type': 'cmd', 'cmd': cmd}));
```

C'est donc un cran *en dessous* d'un miroir : même pas de classe Dart à tenir à
jour, juste des chaînes littérales (`'type'`, noms de champs) répétées à chaque
site d'appel. Rien ne garantit que `'type': 'add_bot'` correspond bien au tag
`snake_case` que serde dérive de `AddBot` — la correspondance n'est vraie que
par convention et par test (`client_message_wire_format_is_stable`, côté Rust
seulement).

### 2.2 Messages serveur → client (`ServerMessage`, 14 variantes)

Symétrique : Rust définit `RoomCreated`, `Joined`, `Spectating`, `Lobby`,
`GameStarted`, `Update`, `Rejected`, `Error`, `Mods`, `Queued`, `MatchFound`,
`Rating`, `RatingsUpdated`, `Pong`. Dart les décode via un unique `switch
(msg['type'])` dans `GameSession._handle` (session.dart:416-511), avec accès
champ par champ non typé (`msg['code'] as String`, `msg['view'] as
Map<String, dynamic>?`).

Point notable : **ce switch n'a pas de `default`**. Un type de message inconnu
ne lève rien, ne logue rien — il tombe silencieusement au sol. C'est exactement
le mécanisme qui rend une nouvelle variante `ServerMessage` ajoutée côté serveur
invisible côté client tant que personne n'ajoute le `case` correspondant à la
main.

Trois messages (`Queued`, `MatchFound`, `Rating`/`RatingsUpdated`, ADR-0034) ne
sont **pas du tout consommés côté Flutter** — le roadmap du projet le confirme
(« ranked menu greyed until a matchmaking service exists »). C'est une surface
de miroir latente : le jour où le classement est câblé côté client, quatre
nouvelles formes devront être ajoutées au miroir, avec le même risque.

### 2.3 Commandes de jeu (`CommandKind`, 18 variantes) — la duplication la plus dangereuse

Rust (`crates/engine/src/command.rs`) : `PlayMovementCard`, `Build`,
`ProposeTrade`, `AcceptTrade`, `DeclineTrade`, `CancelTrade`,
`SubmitBlindBid`, `SellHouse`, `Expropriate`, `BoostRent`, `Mortgage`,
`Unmortgage`, `ChooseLegalRoute`, `OfferBribe`, `VoteOnBribe`, `Resign`,
`EndTurn`, `UseJailCard`.

Côté Dart, ces commandes sont construites comme des `Map` littéraux **dispersés
dans six fichiers d'UI différents** (`ui/side/side_panel.dart`,
`ui/side/trade_dialog.dart`, `ui/game/game_screen.dart`,
`ui/game/actions_panel.dart`, `ui/game/nav_rail.dart`), chacun retapant le tag
et les noms de champs :

```dart
s.sendCmd({'type': 'build', 'tile': def.id});
s.sendCmd({'type': 'submit_blind_bid', 'amount': (int.tryParse(_bid.text) ?? 0).clamp(0, cash)});
s.sendCmd({'type': 'choose_legal_route', 'order': _routeOrder});
```

C'est la duplication structurellement la plus risquée du lot : il n'y a même
pas un point de rassemblement (contrairement à 2.1 où tout passe au moins par
`session.dart`). Un renommage de champ (`tile` → `tile_id`, par exemple) doit
être retrouvé et corrigé dans cinq fichiers de widgets indépendamment, sans
qu'aucun compilateur Dart ne s'en aperçoive — l'erreur ne se manifeste qu'au
runtime, sous forme de `Rejected` silencieusement absorbé ou d'un comportement
incorrect.

### 2.4 Événements (`Event`, 44 variantes) — miroir dupliqué **deux fois** côté Dart

C'est le cas le plus intéressant : le même enum Rust est retranscrit **deux
fois indépendamment** côté Dart, pour deux usages différents, avec deux niveaux
de couverture différents :

- `describeEvent()` (protocol.dart) — 44 `case` correspondant à peu près à
  toutes les variantes, transforme l'événement en ligne de log localisée.
- `_beatsFor()` (director.dart) — 29 `case` seulement, transforme l'événement
  en séquence d'animations (ADR-0030).

Les deux ont un comportement de repli **silencieux et différent** :
- `describeEvent` : `default: return e.toString();` → une variante non gérée
  s'affiche comme un dump Dart brut du `Map` JSON dans le log (moche mais
  visible).
- `_beatsFor` : `default: return const [];` (commentaire dans le code : *"P4:
  never a beat"*) → une variante non gérée ne produit **aucune animation, sans
  erreur, sans log**. Un nouvel `Event` ajouté côté Rust et jamais câblé côté
  `director.dart` est un bug purement silencieux : le jeu continue de
  fonctionner, l'état est correct, mais rien ne se passe visuellement à la
  table pour cet événement précis.

Deux miroirs indépendants du même enum, avec deux taux de couverture
différents (44 vs 29) : c'est la preuve la plus concrète que « le protocole est
défini deux fois » comprise dans son énoncé le plus littéral — ici il est même
défini *trois* fois en comptant le Rust.

### 2.5 Erreurs de rejet (`CommandError`, 35 variantes)

Rust (`crates/engine/src/error.rs`) : 35 variantes typées, sérialisées
`#[serde(tag = "code", rename_all = "snake_case")]`. Dart (`rejectReason()`,
protocol.dart) : un switch de 35 `case` qui traduit chaque code en message
localisé. Repli : `default: return code;` — un code non reconnu s'affiche tel
quel (`"bid_below_floor"`) à l'utilisateur au lieu du texte localisé. Dégradation
silencieuse mais moins grave (le joueur voit quelque chose, juste moche et non
traduit) — comparable à D8 dans `docs/technical-debt.md`, mais jamais formalisé
comme entrée de dette à ce jour.

### 2.6 Structures de données partagées (16 types mirrorés à la main)

Chacune de ces classes Dart porte un constructeur `.fromJson` écrit à la main,
champ par champ, avec des casts non vérifiés (`j['x'] as int`, `as String?`) :
`SeatInfo`, `TileDef`, `MarketEventDef`, `GameContent` (miroir de
`ResolvedContent`/`ModInfo`), `RuleParams`, `RoomSettings`, `PlayerView`,
`TileState`, `TurnPhase`, `TradeOffer`, `ScheduledEvent`, `ActiveMarketEvent`,
`MarketForecast`, `Spotlight`, `ClientView`, `RatingChange` (non consommé
aujourd'hui, cf. 2.2).

Chaque champ Rust ajouté, renommé ou retypé sur `RuleParams`, `ClientView`,
`PlayerView`, etc. doit être répercuté à la main sur son homologue Dart. Le
cast `j['champ'] as int` ne « rate » pas silencieusement une absence de champ
grâce aux `??` défensifs déjà présents partout — mais c'est une discipline
appliquée manuellement à chaque ligne, pas une garantie structurelle.

### 2.7 Constantes numériques dupliquées

Recensées par grep croisé entre `crates/server/src/room.rs` et
`clients/flutter/lib/*.dart` :

| Constante Rust | Valeur | Copie Dart | Fichier |
|---|---|---|---|
| `BID_WINDOW` (room.rs:62) | 12 s | `Duration(seconds: 12)` | session.dart:171 |
| `VOTE_WINDOW` (room.rs:67) | 5 s | `Duration(seconds: 5)` | session.dart:174 |
| `JAIL_DECISION_SECS` (room.rs:54) | 20 s | `_jailDecisionSecs = 20` | session.dart:137 |
| `ANIM_ACK_CAP` (room.rs:81) | 10 s | budget des tiers d'animation (8s/6s/4s) | motion.dart, ADR-0030 |

Ce ne sont pas des valeurs protocolaires au sens strict (elles ne voyagent pas
sur le fil), mais ce sont des **contrats temporels implicites** entre serveur
et client : le commentaire de `session.dart:167` le dit lui-même — *« a local
approximation of the server's window [...] not a precise mirror »*. Si le
serveur change `BID_WINDOW`, rien ne casse à la compilation ni aux tests ; le
client affiche juste un compte à rebours visuellement faux.

À cela s'ajoutent deux formules déjà connues et documentées comme dette **D8**
dans `docs/technical-debt.md` (`GameState::net_worth` vs `session.dart::netWorth()`,
et `Exec::market_price` vs `protocol.dart::marketPrice()`) — ce sont des
sous-cas du même problème général que H1, pas des dettes séparées à traiter en
parallèle.

### 2.8 Sérialisation/désérialisation manuelles

Chaque `.fromJson` listé en 2.6, chaque `Map` construit à la main en 2.1/2.3,
et l'unique point d'encodage `jsonEncode(...)` (pas de couche de sérialisation
partagée) constituent la totalité de la couche de (dé)sérialisation Dart :
entièrement écrite à la main, aucune génération, aucune validation de schéma à
la réception. Une valeur du mauvais type envoyée par un serveur (bug, ou
serveur tiers auto-hébergé légèrement en avance/retard de version) provoque un
`TypeError` Dart non rattrapé au point de cast, pas une erreur de
désérialisation propre.

### 2.9 Le trou de CI qui rend tout ça silencieux

C'est la pièce manquante qui transforme une duplication ordinaire en dette
**« drift silencieux »**, telle que nommée par l'énoncé :

```yaml
# .github/workflows/ci.yml (Rust)
paths-ignore:
  - "clients/flutter/**"   # ...entre autres

# .github/workflows/flutter.yml
paths:
  - "clients/flutter/**"   # seul déclencheur
```

Une pull request qui modifie uniquement `crates/protocol/src/lib.rs` ou
`crates/engine/src/{command,event,error}.rs` (ajout d'une variante, renommage
d'un tag serde, ajout d'un champ) déclenche `ci.yml` et **jamais**
`flutter.yml`. `flutter analyze`, `flutter test` et `flutter build web` ne
tournent tout simplement pas. La PR peut être verte et mergée sans qu'aucun
outil n'ait even *tenté* de compiler le code Dart qui référence potentiellement
l'ancien tag.

Ce découplage de CI est un choix délibéré et documenté (commentaire dans
`flutter.yml` : *"Rust-only changes never pay the Flutter SDK setup"*) —
raisonnable en soi pour la vitesse de CI, mais qui a un angle mort exact sur le
protocole partagé. **Toute stratégie retenue doit refermer ce trou**,
indépendamment du mécanisme de duplication choisi par ailleurs.

### 2.10 Récapitulatif chiffré

| Catégorie | Variantes/champs Rust | Mécanisme Dart | Fichiers Dart concernés |
|---|---|---|---|
| `ClientMessage` | 17 | `Map` littéraux ad hoc, aucun type | session.dart + 5 fichiers UI |
| `ServerMessage` | 14 | 1 switch sans `default` | session.dart |
| `CommandKind` | 18 | `Map` littéraux ad hoc, aucun type | 5 fichiers UI |
| `Event` | 44 | 2 switches indépendants (44 et 29 cas) | protocol.dart, director.dart |
| `CommandError` | 35 | 1 switch, repli sur le code brut | protocol.dart |
| Structures de vue | 16 classes | 16 `.fromJson` manuels | protocol.dart |
| Constantes de timing | 4 | valeurs recopiées | session.dart, motion.dart |
| **CI** | — | trigger disjoint Rust/Flutter | `ci.yml` / `flutter.yml` |

---

## 3. Stratégies possibles

Pour chaque stratégie : fonctionnement, avantages, inconvénients, effort,
risques, impact architecture.

### S1 — Discipline outillée : tests de conformité "golden" + fermeture du trou de CI (pas de génération)

**Fonctionnement.** On ne touche pas au fait que le miroir Dart soit écrit à la
main. On ajoute un répertoire de fixtures JSON versionnées (`tests/protocol-fixtures/`
par exemple, une valeur canonique par variante des 5 enums). Un test Rust
vérifie que `serde_json::to_string`/`from_str` produit exactement ces fixtures
(extension de ce qui existe déjà dans `crates/protocol/src/lib.rs#tests`,
généralisé à *toutes* les variantes, pas seulement celles couvertes
aujourd'hui). Un test Dart (`flutter test`) charge les mêmes fixtures et
vérifie que chaque `.fromJson`/switch les traite sans tomber dans un `default`
silencieux (on ajoute des assertions explicites « ce type est géré » plutôt que
de laisser passer le cas par défaut). On corrige `ci.yml`/`flutter.yml` pour
qu'un changement dans `crates/protocol/**` ou `crates/engine/src/{command,event,error}.rs`
déclenche aussi `flutter.yml` (ou qu'un job léger dédié aux fixtures tourne
dans les deux CI, sans exiger le SDK Flutter complet à chaque fois).

**Avantages.** Aucune nouvelle dépendance, aucun outil de génération, angle
mort de CI refermé immédiatement. Rend le drift *bruyant* : toute variante
oubliée fait échouer un test dans la même PR qui l'introduit, côté Rust comme
côté Dart. Coût d'entrée très faible, compréhensible par n'importe quel
contributeur OSS sans apprendre un nouvel outil.

**Inconvénients.** Ne supprime aucune des six duplications recensées — elle les
rend seulement détectables. La double frappe reste nécessaire à chaque
évolution (ajouter une variante `Event` exige toujours de mettre à jour
`describeEvent`, `_beatsFor`, et maintenant *aussi* la fixture — un peu plus de
travail, pas moins). Les fixtures elles-mêmes doivent être tenues à jour à la
main ; un contributeur pressé peut être tenté de les dupliquer plutôt que de
les faire échouer correctement.

**Effort.** Petit à moyen : un répertoire de fixtures, une extension du test
Rust existant, un test Dart de couverture par switch, une correction des
chemins de trigger CI.

**Risques.** Faibles — purement additif, aucun changement de runtime.

**Impact architecture.** Nul. Couche de test/CI uniquement.

### S2 — Génération légère du miroir Dart depuis les types Rust (schéma intermédiaire + petit générateur maison)

**Fonctionnement.** On dérive `schemars::JsonSchema` sur les types déjà
`Serialize`/`Deserialize` du protocole (`ClientMessage`, `ServerMessage`,
`CommandKind`, `Event`, `CommandError`, `RuleParams`, `ClientView`, etc. —
aucune réécriture de leur définition, juste un derive de plus). Un petit
binaire `xtask` (pur Rust, dans le workspace, pas de nouvel outil externe)
appelle `schema_for!` sur chaque type et émet directement un fichier Dart
généré (`protocol.g.dart` : classes scellées `sealed class`/`fromJson`, plus un
constructeur typé par variante de commande — fini les `Map` littéraux
dispersés). Le fichier généré est commis (même logique que `gen-l10n`, déjà en
usage dans le projet) et un check CI (`cargo run -p xtask -- gen-dart --check`)
échoue si le fichier commis diverge de ce que régénèrent les types actuels.
Comme le générateur est un binaire Rust pur, ce check tourne naturellement
dans `ci.yml` (le job Rust), sans exiger le SDK Flutter — ce qui referme le
trou de CI (2.9) *gratuitement*, sans dépendre de la fermeture séparée décrite
en S1.

Recherche effectuée : à ma connaissance il n'existe pas d'équivalent mûr et
largement adopté de `ts-rs`/`specta` ciblant Dart (ces outils existent pour
TypeScript). Le générateur serait donc forcément un petit outil maison — mais
un outil maison *au-dessus* d'une introspection de schéma existante et fiable
(schemars), pas un parseur de code Rust fait main.

**Avantages.** Élimine réellement la duplication structurelle (2.1, 2.2, 2.3,
2.6) : les types Dart cessent d'exister indépendamment, ils *sont* dérivés du
Rust. Les switches Dart sur `Event`/`CommandError` (2.4, 2.5) peuvent devenir
des `switch` exhaustifs sur un `sealed class` généré au lieu de `switch` sur
`String` avec repli silencieux — Dart 3 refuse de compiler un switch non
exhaustif sur un `sealed class` : le point le plus dangereux de tout l'audit
(2.4, le `default: return const [];` invisible de `director.dart`) devient une
**erreur de compilation** au lieu d'un bug de production silencieux. C'est le
seul scénario qui transforme réellement le "silencieux" de l'intitulé de la
dette en "impossible à ignorer".

**Inconvénients.** Reste un vrai projet d'outillage à concevoir et maintenir
(mapping des enums externally-tagged serde vers des `sealed class` Dart
idiomatiques, gestion de `#[serde(default)]`/`skip_serializing_if`, etc. — la
partie non triviale n'est pas l'extraction du schéma mais la qualité de l'émission
Dart). Ajoute une dépendance `schemars` au workspace (à valider sous
`cargo deny`/`cargo machete`, licence MIT/Apache donc a priori conforme). La
première migration est une PR large (protocol.dart, session.dart, et les 6
fichiers UI qui construisent des commandes en `Map`) — pas dangereuse
techniquement (le compilateur Dart guide la migration) mais volumineuse à
relire. Ne supprime **pas** la duplication sémantique restante (le texte
localisé de `describeEvent`, le mapping événement→animation de `director.dart`
doivent toujours exister à la main quelque part) — seulement l'exhaustivité en
devient vérifiée par le compilateur.

**Effort.** Moyen à grand une fois (concevoir + écrire le générateur, migrer
l'existant), puis faible en continu (régénération automatique à chaque
`cargo run -p xtask -- gen-dart`).

**Risques.** Moyens : bugs de génération sur les cas serde les plus subtils
(enums à charge utile mixte, `Option` vs champ absent), régressions possibles
lors de la migration d'une grosse PR touchant beaucoup de fichiers UI —
atténuable en migrant type par type plutôt qu'en un seul big-bang.

**Impact architecture.** Ajoute une étape de build analogue à `gen-l10n`
(précédent déjà accepté dans ce projet). Ne touche pas au format du fil
(toujours du JSON `snake_case` — aucun ADR protocolaire remis en cause). Le
crate `protocol` gagne une dépendance de dev/build (`schemars`).

### S3 — IDL binaire partagé (Protobuf / FlatBuffers) avec génération officielle Rust + Dart

**Fonctionnement.** Le schéma canonique devient un fichier `.proto` (ou
équivalent) ; `prost` génère les types Rust, le générateur officiel `protoc`
génère les types Dart ; le protocole WebSocket passe de JSON à un encodage
binaire.

**Avantages.** Outillage mûr des deux côtés (contrairement à S2 où le côté Dart
serait maison), enums exhaustifs générés nativement dans les deux langages,
payloads plus compacts.

**Inconvénients — rédhibitoires ici.** Remet en cause un invariant documenté du
projet : *« Wire-format tests exist; changing serde shapes is a protocol
break »* et *« the wire format IS the replay format »* (CLAUDE.md, section
protocole). Le format actuel est aussi ce qui rend le protocole
inspectable/debuggable directement dans les devtools du navigateur pendant le
développement d'un projet hobby/OSS — un vrai atout perdu avec du binaire.
Nécessite un toolchain externe supplémentaire (`protoc`, pas juste
Cargo/Flutter) à épingler dans le devcontainer et toutes les CI — c'est
*l'inverse* de la priorité « builds reproductibles » et « architecture
simple » explicitement demandée : on remplace un problème de duplication de
code par un problème de duplication *et* de dépendance à un toolchain tiers.
Réécriture cross-cutting du serveur (`ws.rs`), du CLI, et du client — un
chantier sans rapport avec la taille réelle du problème (des messages JSON de
petite taille sur un jeu de plateau, pas un système à hautes performances).

**Effort.** Très grand.

**Risques.** Élevés — régressions transverses, breaking change du protocole
public que des serveurs communautaires pourraient déjà exposer (le modèle
« Minecraft » de serveurs auto-hébergés indépendants rend un breaking change de
protocole coûteux socialement, pas seulement techniquement).

**Impact architecture.** Très grand ; contredit plusieurs priorités énoncées.
Présentée pour complétude, mais à écarter.

### S4 — Compiler `protocol`/`engine` en WebAssembly et lier Flutter dessus (FFI/WASM), au lieu de le miroiter

**Fonctionnement.** `wasm-bindgen`/`wasm-pack` pour le web (`dart:js_interop`),
et `flutter_rust_bridge`/`cbindgen` + `dart:ffi` pour le desktop (Windows,
Linux, macOS) — deux mécanismes de liaison différents car WASM ne couvre que le
web.

**Avantages.** La duplication disparaît par construction : c'est le vrai code
Rust qui tourne aussi côté client, plus de deuxième implémentation.

**Inconvénients — rédhibitoires ici.** L'ADR-0025 a délibérément choisi *« un
seul codebase Dart pour desktop et web »* — solution simple et déjà en
production. Cette stratégie la fracture en deux chemins d'intégration
(WASM web-only + FFI desktop-only), donc *augmente* la complexité
architecturale au lieu de la réduire, à l'exact opposé de la priorité
« architecture simple ». Même en l'adoptant, les widgets Flutter ont toujours
besoin de types Dart idiomatiques (`ClientView`, etc.) pour se binder — on finit
très probablement par réécrire une fine couche Dart autour du WASM/FFI de
toute façon, donc le miroir ne disparaît pas vraiment, il se déplace et se
complexifie. Nouveau toolchain lourd (deux chaînes de liaison distinctes) pour
un problème qui ne concerne au fond que la (dé)sérialisation de petits messages
JSON.

**Effort.** Très grand, deux chantiers d'intégration distincts.

**Risques.** Élevés — nouvelle classe de bugs (marshaling FFI/WASM), surface de
plantage sur des plateformes déjà notées comme fragiles (le flux OIDC web est
déjà listé comme "jamais testé en conditions réelles" dans les rough
surfaces du projet).

**Impact architecture.** Très grand ; défait un choix ADR récent et bien
fonctionnant pour un bénéfice marginal par rapport à S2. Présentée pour
complétude, mais à écarter.

---

## 4. Tableau comparatif

| | S1 — tests golden + fix CI | S2 — génération légère (schemars + xtask) | S3 — IDL binaire | S4 — WASM/FFI |
|---|---|---|---|---|
| Source unique de vérité | Non (duplication détectée, pas supprimée) | Oui (types Dart dérivés du Rust) | Oui | Oui |
| Génération de code | Aucune | Faible, maison, ciblée | Lourde, outillage externe | Très lourde, deux toolchains |
| Simplicité architecture | Inchangée | +1 étape de build (type gen-l10n) | Gros changement | Gros changement, fracture desktop/web |
| Build reproductible | Inchangé | Oui (Cargo seul) | Nouveau binaire externe requis | Deux toolchains natifs supplémentaires |
| Maintenance OSS | Facile, rien de nouveau à apprendre | Modérée (comprendre le générateur) | Difficile (protoc, breaking wire) | Difficile (FFI/WASM) |
| Ferme le trou de CI (2.9) | Oui, explicitement | Oui, comme effet de bord | Oui | Oui |
| Rend le silencieux "compilable" | Non (juste testé) | Oui (`sealed class` exhaustif) | Oui | Oui |
| Effort | Petit-moyen | Moyen-grand puis faible | Très grand | Très grand |
| Risque | Faible | Moyen | Élevé | Élevé |
| Cohérent avec les priorités énoncées | Oui | Oui | Non | Non |

---

## 5. Recommandation

**Recommandation en deux phases, pas un choix binaire : S1 puis S2.**

**Phase 1 (immédiate) : S1.** Ajouter les fixtures golden et corriger le
déclenchement de CI. C'est peu coûteux, ne demande aucune décision
d'architecture engageante, et referme *tout de suite* le trou le plus grave de
l'audit (2.9) : le fait qu'un changement de protocole Rust puisse aujourd'hui
merger sans qu'aucun outil Dart ne l'ait vu. Même si S2 est retenue ensuite,
ce travail n'est pas perdu : les fixtures deviennent les vecteurs de test du
générateur.

**Phase 2 (à programmer) : S2.** C'est la seule stratégie qui répond
réellement à l'objectif énoncé — *« une seule source de vérité »* — sans
sacrifier aucune des priorités listées (architecture simple, très peu de
génération, builds reproductibles avec Cargo seul, maintenance facile pour un
projet OSS). Le point décisif en sa faveur, au-delà de la suppression de la
duplication : elle transforme le pire cas trouvé dans cet audit — le
`default: return const [];` silencieux de `director.dart` (2.4), où un
événement serveur non traité ne casse rien et n'affiche rien — en une erreur de
compilation Dart via l'exhaustivité des `sealed class`. C'est la différence
entre « le protocole est testé » (S1) et « le protocole ne peut pas diverger
sans que `flutter analyze` refuse de compiler » (S2), ce qui est la formulation
la plus proche de « source unique de vérité » qu'on puisse obtenir sans casser
le format JSON ni fracturer l'architecture client desktop+web actuelle.

S3 et S4 sont écartées : toutes deux résolvent le problème en apparence plus
« proprement » (un seul vrai code exécuté, ou un IDL mûr des deux côtés), mais
au prix d'un changement d'architecture largement disproportionné par rapport à
la taille réelle du problème — quelques dizaines de types JSON pour un jeu de
plateau — et en contradiction directe avec au moins deux priorités explicitement
données (« architecture simple », « très peu de génération de code »).

**Pourquoi ne pas faire S2 directement sans S1 ?** Parce que S2 est un projet
non trivial (concevoir le générateur, migrer six fichiers UI) qui prendra du
temps avant d'être mergé, pendant lequel le trou de CI (2.9) reste ouvert. S1
est mergeable en une petite PR et ferme ce trou dès aujourd'hui, indépendamment
du calendrier de S2.

---

## 6. Ce que je n'ai *pas* fait

Conformément à la demande, aucun code n'a été modifié. Ce document n'a pas non
plus été ajouté à `docs/AI_ENGINEERING.md` ni référencé depuis
`docs/technical-debt.md` — à faire une fois la stratégie validée (et le
document traduit en anglais s'il doit vivre dans `docs/` de façon durable).
