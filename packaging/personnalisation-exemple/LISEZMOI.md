# Personnaliser MMail à l'installation

Un dossier `personnalisation` donne à MMail l'icône, le logo et les couleurs d'une organisation.
Chaque fichier est facultatif :

| Fichier | Effet |
|---|---|
| `personnalisation.json` | `nom` : nom de l'apparence (40 caractères au plus) ; `apparence` : couleurs, de la même forme que « Créer mon apparence… » ; `imposer` : `true` pour qu'elle soit la seule au choix |
| `icone.ico`, `icone.png` ou `icone.svg` | icône des fenêtres et de la barre des tâches (1 Mio au plus) ; `icone.ico` sert aussi aux raccourcis sous Windows |
| `logo.svg` ou `logo.png` | logo en tête du bandeau, à la place du signe de MMail (2 Mio au plus) |

`apparence` :

- `base` : l'apparence de départ, parmi `mmedia`, `pomme`, `outlook`, `moderne`, `nuit`, `classique` ;
- couleurs en `#RRGGBB` : `bandeau`, `accent`, `surbrillance`, `fond`, `lien`.

MMail règle lui-même la couleur des textes posés dessus, pour qu'ils restent lisibles. Une couleur
invalide est écartée, et le journal de MMail le dit.

Le bandeau, et donc le logo, n'apparaît qu'avec une base qui en porte un (`mmedia`), ou si
`bandeau` est donné.

## Où MMail le trouve

1. Le réglage du poste `Personnalisation`, ou la variable d'environnement `MMAIL_PERSONNALISATION` :
   le chemin du dossier, par exemple un partage réseau pour tout un parc. Sous Windows, le réglage
   se pose dans `HKLM\Software\M-Media\MMail`.
2. Sinon, `personnalisation` à côté du programme : dans le dossier d'installation sous Windows, ou à
   côté du fichier `.AppImage` sous Linux.

## À l'installation, sous Windows

Placez le dossier `personnalisation` à côté de l'installeur, puis lancez celui-ci. Le dossier est
copié avec MMail. Une mise à jour ultérieure le garde.

L'apparence de l'organisation est celle d'un nouveau profil. Une personne qui avait déjà choisi la
sienne la garde, sauf si `imposer` est vrai.

Android n'est pas encore pris en charge.
