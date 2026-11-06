# Journal des versions — Live Régie

Toutes les évolutions notables de **Live Régie**, la régie compagnon de TikTok LIVE Studio.

Numérotation `MAJEUR.MINEUR.CORRECTIF` :
- **correctif** (`2.1.x`) : corrections de bugs, sans changement de fonctionnement ;
- **mineur** (`2.x.0`) : nouvelles fonctionnalités, compatibles avec les réglages existants ;
- **majeur** (`x.0.0`) : changement qui casse la compatibilité (par exemple d'anciens fichiers de réglages qui ne se lisent plus).

Chaque version est marquée par un tag Git (`v2.1.0`…), ce qui permet d'y revenir à tout moment.

---

## [2.3.0] — 2026-11-06

### Numéros d'urgence
- **L'encart s'étire maintenant vers la gauche et la droite en 16:9.** Les emplacements « Gauche », « Droite » et « Bandeau » étaient imposés en force, avec une largeur limitée à 380 px, et annulaient le redimensionnement à la souris. Ce ne sont plus que des positions de départ ; un clic sur l'un de ces boutons replace l'encart.
- **Nouveaux réglages** (volet 🛡️ Sécurité et fenêtre de configuration) :
  - taille du texte jusqu'à **250 %** (au lieu de 160 %), titre compris ;
  - nombre de **colonnes** : Auto, 1, 2, 3 ou 4 ;
  - **descriptions en entier** : retour à la ligne au lieu de « … » (activé par défaut).
- En 16:9, le nombre de colonnes s'adapte automatiquement à la largeur de l'encart.

## [2.2.2] — 2026-11-06

### Corrections
- **Le chat ne se reconnectait plus après un redémarrage de la régie.** La régie arrête le pont chat à sa fermeture, pour éviter les doublons, mais ne le relançait pas.
  - Elle **se connecte maintenant automatiquement au démarrage** dès qu'un pseudo est renseigné. Sans live en cours, elle réessaie toutes les 30 s.
- **Le voyant « ○ Chat »** de la barre du haut est cliquable : un clic lance la connexion.
- **Pas de pont en double après un plantage :** le numéro de processus du pont est noté dans `chat_bridge.pid`. Au démarrage, un ancien pont encore actif est arrêté, après avoir vérifié que c'est bien un Python.
- Le pseudo du chat est vide par défaut sur une nouvelle installation (il valait « Et si ... »).

## [2.2.1] — 2026-11-06

### Corrections
- **La scène ne se mettait plus à jour quand elle était cachée derrière TikTok LIVE Studio.** Par exemple, un changement de branche (F9 à F11) n'apparaissait qu'en revenant sur la fenêtre de scène.
  - Cause : Edge, déjà lancé en arrière-plan (démarrage rapide de Windows), ouvrait la scène dans son processus existant et **ignorait les options anti-veille**. La page cachée se mettait donc en veille.
  - Correctif : les scènes, la fenêtre presse et la fenêtre urgences s'ouvrent maintenant dans un **Edge dédié** (profil `edge_scene_profile/`), qui applique réellement ces options. Ton Edge habituel n'est pas touché.

## [2.2.0] — 2026-11-06

### Interface réorganisée
- **5 onglets rangés selon le moment d'utilisation**, au lieu de 7 onglets thématiques :
  - **🔴 Direct** : chrono de débat, branche active, textes de la scène, sons et jingles ;
  - **🖼️ Contenus** : vidéo (YouTube ou fichiers, préréglages YouTube en volet), annonce, revue de presse, musique détectée ;
  - **👥 Public** : chat et questions épinglées, jukebox, sondages, jauge de likes ;
  - **📝 Bilan** : demandes d'aide, journal du live ;
  - **⚙️ Réglages** : volets repliables Branches, Mise en page des scènes, Sécurité (numéros d'urgence, avertissement), Remerciements, Comptes TikTok et auto-tappeur.
- **Voyants dans la barre du haut** :
  - **● / ○ Chat** indique l'état de la connexion ;
  - **🆘 N** compte les demandes d'aide à traiter ; un clic ouvre le Bilan.
- **Plus de doublons** : l'encart d'annonce, présent à la fois dans Régie et dans Bannières, n'apparaît plus qu'une fois. Le lecteur vidéo et l'onglet YouTube sont réunis dans Contenus.
- **Plus de numéros de section** (1, 4, [4], 2, 3, 5…), et des titres qui disent ce que fait chaque bloc.

### Corrections
- Le statut du chat chevauchait le bouton « Configurer le Bot ». Il s'affichait aussi en **vert** quand il était « Déconnecté », parce que le mot contient « connect ».
- Les boutons d'onglets, de branches et de bascules débordaient à droite de la fenêtre (l'espacement entre boutons n'était pas compté).

## [2.1.3] — 2026-11-06

### Corrections
- **La scène 16:9 disparaissait de la liste des fenêtres de TikTok LIVE Studio.** Elle s'ouvrait en plein écran et recouvrait TikTok ; on la réduisait, et une fenêtre réduite ne peut plus être capturée.
  - Elle n'est plus en plein écran : la régie la place et la dimensionne elle-même à 80 % de l'écran, car Edge ignore la taille demandée quand il tourne déjà en arrière-plan. La scène 9:16 est placée de la même façon.
  - La régie affiche un bandeau « La scène est réduite », avec un bouton **Afficher la scène**.
- **Les deux scènes portaient le même nom de fenêtre.** Elles s'appellent maintenant `STUDIO_LIVE_TIKTOK 9:16` et `STUDIO_LIVE_TIKTOK 16:9`, et se distinguent dans la liste des fenêtres.

## [2.1.2] — 2026-11-06

### Améliorations
- **Le contenu des encarts s'adapte à leur taille.** L'annonce, les numéros d'urgence, l'avertissement et la question épinglée prennent automatiquement la plus grande taille qui tient dans le cadre, recalculée pendant le redimensionnement et quand le texte change.
  - La grille des numéros d'urgence choisit seule son nombre de colonnes.
  - Réglable dans Préférences → « Mise en page des scènes » (case à cocher).
- **La vidéo remplit son encart** quand on lui donne une hauteur, en mode « Cadre entier » ou « Remplir ».
- **Mise en page enregistrée par la régie** (position, largeur **et hauteur** de chaque encart, pour chaque format). Elle est identique dans TikTok LIVE Studio, OBS et le navigateur, et résiste aux relances et au vidage du cache. Elle se réinitialise depuis la régie.

### Corrections
- La hauteur des encarts n'était pas sauvegardée en 16:9.
- Déplacer un encart effaçait la position enregistrée des encarts masqués à ce moment-là.

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
