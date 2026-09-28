# MMail

Client de messagerie IMAP pour Windows, Linux et Android. Il remplace Outlook
sur le poste dans le cadre de la sortie de Microsoft 365.

Noyau en **Rust**, interface en **Qt 6 / QML**, sous **GNU GPL v3**.

## Ce qu'il cherche à faire

**La boîte n'est pas une cloison.** Tous les clients existants, libres ou
payants, partent du compte : une arborescence par compte, et le déplacement d'un
message vers le dossier d'une autre boîte va du pénible à l'impossible. MMail
part du message.

Le cadrage complet — exigences, décisions actées (les « décisions » numérotées
citées dans le code), pistes écartées et leur motif — est tenu dans un dossier
de projet interne.

## État — 0.1.2

**Lecture et tri.** Pas encore de rédaction ni d'envoi.

- **plusieurs comptes dans une seule arborescence**, repliables, avec la
  rubrique **Favoris** au-dessus : on l'alimente en y **glissant un dossier**,
  et l'on réordonne ses favoris de la même façon ; un blanc et un trait
  séparent chaque compte de ce qui le précède ;
- **menu « Comptes »** : ajouter un compte — le serveur est trouvé d'après
  l'adresse quand le domaine publie sa configuration —, en ajouter un ou
  plusieurs par un **lien de configuration**, ou en retirer un (voir
  « Configuration d'un compte ») ;
- **menu « ? »** : aide (F1) et « À propos » — versions de MMail, du noyau et
  de Qt ;
- **déplacement par glisser-déposer ou par clic droit**, vers un dossier de la
  même boîte ou de n'importe quelle autre boîte connectée ; dialogue
  « Déplacer vers… » avec filtre (Ctrl+Maj+V) ;
- **Supprimer** envoie à la corbeille de la boîte (touche Suppr) ;
- marquage lu / non lu (Ctrl+Q, Ctrl+U) ; un message affiché est marqué lu ;
- **masquage local** des dossiers peu utilisés, réaffichage en un clic ;
- affichage du **message brut** (bouton « Source ») ;
- **copie automatique de la sélection**, comme dans MMdedit : un texte
  sélectionné dans un message part au presse-papier, sauf ce qu'un autre
  logiciel vient d'y déposer, protégé une minute ; clic droit sur le message :
  « Copier », « Tout sélectionner » ; sur une ligne de la liste : copier
  l'adresse de l'expéditeur ou l'objet ;
- **barre d'information** : compte et dossier ouverts, nombre de messages et de
  non-lus, sélection, état de la synchronisation, déplacements en attente ;
- **pièces jointes** listées sous l'en-tête du message : « Ouvrir » avec le
  logiciel du système, « Enregistrer sous… » ; un programme ou un script
  (`.exe`, `.js`, `.bat`…) ne s'ouvre pas depuis MMail, il s'enregistre ;
- **zoom propre à chaque colonne** : Ctrl + molette sur la colonne, ou Ctrl +,
  Ctrl − et Ctrl 0 sur la dernière colonne survolée ; pincement au doigt ;
- **trois apparences**, celles de MMdedit : classique (Windows 9x), moderne
  (M-Media), système — menu « Affichage » ;
- mot de passe confié au **coffre du système**, jamais écrit par MMail ;
- **synchronisation incrémentale** (QRESYNC) : seul ce qui a changé depuis la
  dernière visite est relu ; les compteurs et le dossier ouvert se mettent à
  jour d'eux-mêmes toutes les deux minutes, ou sur F5.

### Le déplacement entre boîtes

Aucun protocole ne déplace un message d'une boîte vers une autre. MMail le lit
en entier à la source, le **copie sur disque**, le dépose dans la cible avec ses
drapeaux et sa date de réception d'origine, et **ne le retire de la source
qu'une fois le dépôt accepté**. Une coupure au milieu ne perd rien : le
déplacement reprend à la connexion suivante, et la cible est d'abord interrogée
par `Message-ID` pour ne pas créer de doublon.

## Configuration d'un compte

### Configuration automatique

À l'ajout d'un compte, dès que l'adresse est saisie, MMail cherche le serveur
IMAP dans le document de configuration que publie le domaine, au format
« autoconfig » de Thunderbird. Il interroge, dans l'ordre :

1. `https://autoconfig.<domaine>/mail/config-v1.1.xml?emailaddress=<adresse>`
2. `https://<domaine>/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress=<adresse>`

Il retient le premier serveur `imap` en `SSL` sur le port 993 dont
l'identifiant est l'adresse elle-même. Aucun annuaire tiers n'est interrogé.
Mailcow sert ce document : il suffit que `autoconfig.<domaine>` désigne le
serveur. Un serveur saisi à la main n'est jamais remplacé.

### Lien de configuration

Un administrateur peut préparer un ou plusieurs comptes, mot de passe compris,
derrière une adresse HTTPS **à usage unique**. La personne la colle dans
« Comptes › Ajouter avec un lien de configuration… », sans rien saisir
d'autre ; les mots de passe vont au coffre du système sans être affichés.

Ce que MMail attend du serveur qui sert le lien :

- MMail l'interroge en **`POST`**, sans corps, avec `Accept: application/json`,
  et ne suit aucune redirection ;
- `200` : le document ci-dessous, que le serveur **détruit en le remettant** ;
  `404` ou `410` : lien inconnu, déjà utilisé ou expiré ;
- un `GET` — la même adresse ouverte dans un navigateur — ne doit pas le
  consommer ; le serveur peut y afficher le mode d'emploi.

```json
{
  "mmail": 1,
  "comptes": [
    { "adresse": "nom@exemple.fr", "hote": "mail.exemple.fr", "motDePasse": "…" }
  ]
}
```

`port`, facultatif, ne peut valoir que 993. Un compte incomplet est écarté, et
signalé ; les autres sont ajoutés. Comme pour une saisie à la main, un mot de
passe n'est confié au coffre qu'une fois accepté par le serveur.

## Limites connues

- Connexion en **IMAPS (port 993)** seulement ; pas de STARTTLS ni d'OAuth2.
- Le corps HTML est affiché **réduit au texte** : paragraphes, blocs, listes
  et lignes de tableau sont respectés, styles et scripts écartés, mais ni
  images ni mise en forme.
- Sous Android, les pièces jointes s'ouvrent mais ne s'enregistrent pas
  ailleurs ; leur ouverture n'a pas été éprouvée sur un téléphone.
- Pas encore de rédaction, de réponse, de signatures ni de filtres.
- Sous Android, les autorités de certification sont celles de Mozilla,
  embarquées dans l'application — et non celles du téléphone.

## Construction

```sh
cmake -S . -B _build -DCMAKE_BUILD_TYPE=Release
cmake --build _build
QT_QPA_PLATFORM=offscreen _build/mmail --smoke
```

Qt 6.4 au minimum, Rust stable, `libsecret-1-dev` sous Linux. Les paquets —
installeur Windows, AppImage, APK — sont produits par l'intégration continue
(`.github/workflows/socle.yml`).

Épreuves contre un vrai serveur, ignorées par défaut (identifiants fournis par
l'environnement, cf. `core/src/essais_serveur.rs`) :

```sh
cargo test --manifest-path core/Cargo.toml -- --ignored --test-threads=1
```

## Licence

GNU General Public License version 3 ou ultérieure — texte intégral dans
[`LICENSE`](LICENSE).
