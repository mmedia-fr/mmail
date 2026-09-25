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

## État — 0.1.0

**Lecture et tri.** Pas encore de rédaction ni d'envoi.

- **plusieurs comptes dans une seule arborescence**, repliables, avec la
  rubrique **Favoris** au-dessus : on l'alimente en y **glissant un dossier**,
  et l'on réordonne ses favoris de la même façon ;
- **déplacement par glisser-déposer ou par clic droit**, vers un dossier de la
  même boîte ou de n'importe quelle autre boîte connectée ; dialogue
  « Déplacer vers… » avec filtre (Ctrl+Maj+V) ;
- **Supprimer** envoie à la corbeille de la boîte (touche Suppr) ;
- marquage lu / non lu (Ctrl+Q, Ctrl+U) ; un message affiché est marqué lu ;
- **masquage local** des dossiers peu utilisés, réaffichage en un clic ;
- affichage du **message brut** (bouton « Source ») ;
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

## Limites connues

- Connexion en **IMAPS (port 993)** seulement ; pas de STARTTLS ni d'OAuth2.
- Le corps HTML est affiché **réduit au texte**.
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
