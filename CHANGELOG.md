# Journal des versions — Live Régie

Toutes les évolutions notables de **Live Régie**, la régie compagnon de TikTok LIVE Studio.

Numérotation `MAJEUR.MINEUR.CORRECTIF` :
- **correctif** (`2.1.x`) : corrections de bugs, sans changement de fonctionnement ;
- **mineur** (`2.x.0`) : nouvelles fonctionnalités, compatibles avec les réglages existants ;
- **majeur** (`x.0.0`) : changement qui casse la compatibilité (par exemple d'anciens fichiers de réglages qui ne se lisent plus).

Chaque version est marquée par un tag Git (`v2.1.0`…), ce qui permet d'y revenir à tout moment.

---

## [2.1.1] — 2026-11-06

### Corrections
- **Jingle vidéo rejoué et figé au milieu de l'écran** :
  - à chaque relance de l'appli, ou à chaque rechargement d'une scène, le dernier jingle était rejoué. Les scènes notent maintenant le dernier jingle existant sans le jouer ;
  - une vidéo de jingle restait affichée si le navigateur refusait sa lecture. Elle est maintenant toujours masquée : à la fin, en cas d'erreur, ou au plus tard à sa durée + 1 s (60 s si la durée est inconnue). Si le son est refusé, elle est lue sans son au lieu de rester bloquée.

## [2.1.0] — 2026-11-06

Première version sous le nom **Live Régie**, avec son logo.

### Nouveautés
- **Nom et logo** : « Live Régie ». Le logo (console de régie + point rouge « en direct ») est intégré aux exe, à la fenêtre et au raccourci de lancement `Live Régie.lnk`. Le numéro de version est affiché en haut de la régie.
- **Branches configurables** (`branches.json`) : nom des boutons, couleurs, étiquette, titre, sujet, sous-titre, mode du bandeau, urgences automatiques et fonds 9:16 / 16:9, modifiables dans l'onglet Préférences. On peut aussi ajouter, réordonner et supprimer des branches.
  - Un seul sélecteur de branches (rangée du haut + mode mini).
  - F9 / F10 / F11 sélectionnent les 3 premières branches.
  - Les scènes suivent la config sans modifier le HTML.
- **Bouclier anti-raid** (F8) : écran de pause opaque sur les scènes ; jingles et épinglages du chat gelés.
- **Chrono de débat** : 1 / 3 / 5 min, +30 s, pause. Pastille sur les scènes (orange à 30 s, rouge à 15 s) et gong à la fin.
- **Remerciements automatiques** : bulle « Merci @pseudo » pour les cadeaux (une seule par série), les partages et les follows.
- **Raccourcis F1 à F4** : sirène, épingler la dernière question, numéros d'urgence, encart d'annonce.
- **File privée des demandes d'aide** : chaque message contenant un mot-clé d'urgence est noté pour recontacter la personne après le live, même s'il contient une insulte. Cette file n'est jamais affichée sur les scènes.
- **Journal du live** : chiffres et chronologie, exportés en Markdown dans `journaux/` avec « Terminer le live ».
- **Reconnexion automatique du chat** : nouvelle tentative après 5 à 60 s ; attente si le compte n'est pas encore en live ; arrêt si la régie est fermée.

### Corrections
- L'onglet **Bannières** (encart d'annonce, bandeau d'avertissement, configuration des numéros d'urgence) n'était plus accessible : il est de retour dans la barre d'onglets.
- Les emojis s'affichaient en `□` dans la régie.
- La branche Chill reprenait les textes de la branche Musique, et son bouton portait une étiquette `[F8]` erronée.
- Les raccourcis clavier pouvaient manquer un appui bref, ou ne plus marcher quand la régie était réduite.
- Le pont chat était lancé en double à chaque clic sur « Connecter », ce qui faisait arriver chaque commentaire en double.
- La jauge de likes de l'auto-tappeur montait sans clic réel.
- La scène 9:16 plantait au changement de format.

### Fiabilité et sécurité
- Sauvegarde de l'état au plus une fois par seconde, au lieu de 4 fichiers à chaque changement.
- Plus aucun chemin `C:\laragon\…` codé en dur : l'appli et son lanceur fonctionnent depuis n'importe quel dossier.
- La route `/assets/` refuse les chemins qui sortent du dossier, et sert WEBP et GIF avec le bon type.
- Les demandes d'aide et le journal sont retirés de `/state`, lisible par n'importe quelle page web locale.

## [2.0.0] — avant 2026-11-06

Version d'origine (« Télécommande Rust — Briser le Silence ») : régie egui, serveur local, scènes 9:16 / 16:9, revue de presse, YouTube, sondages, jingles, pont chat TikTok.
