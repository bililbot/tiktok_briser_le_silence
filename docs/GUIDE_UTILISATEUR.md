<p align="center"><img src="../assets/logo/live_regie_256.png" width="96" alt="Logo Live Régie"></p>

# 📘 Guide utilisateur — Live Régie

**Live Régie** est la régie de ton live **TikTok LIVE Studio** sur PC. Depuis une seule fenêtre, tu pilotes :
- ce qu'affichent tes scènes : thème du live, titres, fonds, vidéos, articles, sondages ;
- les outils de sécurité : numéros d'aide, bouclier anti-raid, avertissement de sujet sensible ;
- le lien avec ton public : chat, questions épinglées, remerciements, demandes d'aide ;
- le rythme de l'émission : chrono de débat, jingles, journal du live.

> Ce guide explique **comment s'en servir**. Le fonctionnement interne est décrit dans la [documentation technique](DOCUMENTATION_TECHNIQUE_STUDIO_TIKTOK.md).

---

## 1. Avant la première utilisation

**Il te faut :**
- Windows 10 ou 11 ;
- **TikTok LIVE Studio** ;
- **Python** et la bibliothèque **TikTokLive**, pour recevoir le chat dans la régie. Une seule fois, dans un terminal :
  ```
  pip install TikTokLive
  ```

**Lancer Live Régie** : double-clique sur le raccourci **Live Régie** (avec le logo) dans le dossier du projet.
- Pour l'avoir aussi sur le Bureau, lance une fois `tools\creer_raccourci.ps1 -Bureau` (clic droit → *Exécuter avec PowerShell*).
- Une seule régie peut être ouverte à la fois. Si tu la relances, l'ancienne est fermée proprement.

---

## 2. Brancher les scènes dans TikTok LIVE Studio

La régie affiche tes scènes à des adresses locales, visibles uniquement sur ton PC :

| Scène | Adresse |
| :--- | :--- |
| Scène verticale 9:16 (format TikTok) | `http://127.0.0.1:8765/stage` |
| Scène paysage 16:9 | `http://127.0.0.1:8765/stage_16_9` |
| Fenêtre revue de presse | `http://127.0.0.1:8765/article` |
| Fenêtre numéros d'urgence | `http://127.0.0.1:8765/urgences` |

**Dans TikTok LIVE Studio** :
- si ta version propose une source de type **lien / page web**, colle directement l'adresse de la scène ;
- sinon, ouvre l'adresse dans Chrome ou Edge, puis ajoute une source **Capture de fenêtre** sur cette fenêtre. Le bouton 🪟 de la régie ouvre les scènes pour toi.

**La fenêtre musique** (`MUSIQUE_LIVE_TIKTOK`, pochette + titre en cours) s'ajoute aussi en **Capture de fenêtre**.

> 💡 La régie doit rester ouverte pendant le live : c'est elle qui fait vivre les scènes. Tu peux la réduire.

---

## 3. Tour de la régie

### La barre du haut (toujours visible)
- **🛡️ BOUCLIER ANTI-RAID (F8)** : le bouton d'urgence en cas d'attaque dans le chat (voir §5).
- **LIVE RÉGIE v2.1.2** : le nom et la version.
- **Format** : bascule entre les scènes 9:16 et 16:9.
- **Mode Mini** : une petite fenêtre avec l'essentiel, à garder dans un coin de l'écran.
- **Les boutons de branches** (💜 VSS & Inceste, ✊ Politique, 🎸 Musique, ☕ Chill…) changent le thème du live. Les 3 premiers ont les raccourcis **F9, F10, F11**.
- **[!] Urg.**, **[TW]**, **[VID]**, **Jukebox** : afficher / masquer les numéros d'urgence, l'avertissement de sujet sensible, la vidéo et le lecteur musique.

### Les onglets
| Onglet | À quoi il sert |
| :--- | :--- |
| **[ Régie ]** | Chrono de débat, remerciements, demandes d'aide, journal du live, branche active, textes du bandeau |
| **[ YouTube ]** | Passer une vidéo YouTube ou une vidéo de ton PC sur la scène |
| **[ Presse ]** | Afficher un article (lien web) dans une fenêtre dédiée qui défile toute seule |
| **🪧 Bannières** | Encart de texte libre, avertissement de sujet sensible, numéros d'urgence |
| **🗳️ Sondages & Chat** | Connexion au chat, questions épinglées, sondages, jauge de likes, jukebox |
| **🎵 Sons** | Boîte à jingles (sons et vidéos courtes) |
| **[ Préférences ]** | Comptes TikTok, **personnalisation des branches**, auto-tappeur |

---

## 4. Préparer un live (check-list)

1. Lance **Live Régie**, puis **TikTok LIVE Studio**, et vérifie que les scènes s'affichent.
2. Onglet **🗳️ Sondages & Chat** : tape ton pseudo, puis clique **⚡ Connecter au Live**.
   - Tu peux le faire **avant** de démarrer le live : la régie se connectera toute seule dès que tu seras en direct.
3. Choisis la **branche** du jour dans la rangée du haut.
4. Si le sujet est sensible, active **[TW]** (avertissement) et vérifie les **numéros d'urgence** (**[!] Urg.**).
5. Clique sur **👁 Aperçu** (bloc Remerciements) pour vérifier que la bulle s'affiche bien sur ta scène.

---

## 5. Pendant le live

### Changer de thème (branche)
Clique sur une branche en haut, ou appuie sur **F9 / F10 / F11**. Le fond, les couleurs, le titre et les textes changent d'un coup.
- Sur une branche comme VSS, les numéros d'urgence s'affichent automatiquement : c'est réglable par branche.
- **Branche active** (onglet Régie) :
  - **↺ Remettre les textes par défaut** annule tes modifications de textes ;
  - **💾 Garder les textes actuels comme défaut** fait de tes textes actuels les textes de la branche.

### Donner la parole : le chrono de débat
Onglet **Régie** → **⏱️ CHRONO DE DÉBAT** :
- écris le nom de la personne (facultatif), puis clique **▶ 1 min**, **▶ 3 min** ou **▶ 5 min** ;
- en cours de route : **+30 s**, **⏸ Pause** / **▶ Reprendre**, **⏹ Stop** ;
- sur la scène, la pastille passe à l'**orange** à 30 s, puis au **rouge** à 15 s. Un **gong** discret sonne à la fin.

### En cas de raid ou de dérapage : le bouclier
Appuie sur **F8** (ou sur le gros bouton rouge) :
- la scène est entièrement **masquée** par un écran « Pause modération », avec les numéros d'aide ;
- le chat ne peut plus déclencher de sons ni épingler de messages ;
- tu peux changer le message du bouclier dans la régie ;
- appuie de nouveau sur **F8** pour reprendre.

> Les demandes d'aide du chat restent prises en compte même pendant le bouclier.

### Mettre une question en avant
- **F2** épingle la **dernière question** posée dans le chat (un message avec un « ? »). Rappuie pour la retirer.
- Tu peux aussi épingler n'importe quel message depuis l'onglet **Sondages & Chat** (bouton **📌**).
- Les spectateurs peuvent proposer une question en écrivant `!question`.

### Remercier ton public
Les remerciements sont **automatiques** : une bulle « 💜 Merci @pseudo pour 5 × Rose ! » apparaît pour les cadeaux, les partages et les follows.
- Une série de cadeaux ne donne qu'une seule bulle, avec le total.
- Tu peux tout couper avec la case **Remerciements auto** (onglet Régie).

### Les demandes d'aide 🆘
Quand quelqu'un écrit un mot comme « aide », « 119 », « 3919 » ou « urgence » :
- les numéros d'aide s'affichent ;
- le message est noté dans **🆘 DEMANDES D'AIDE** (onglet Régie). Cette liste est **privée** : elle n'apparaît jamais sur le live.
- **📋** copie le pseudo, pour retrouver la personne sur TikTok après le live. **✅ Traité** la marque comme traitée.

### Sons, vidéos, articles, sondages
- **Sons** : onglet **🎵 Sons**. **F1** lance la sirène.
- **Vidéo** : onglet **[ YouTube ]**. Colle un lien YouTube ou choisis une vidéo de ton PC. **F12** l'affiche ou la masque.
- **Article** : onglet **[ Presse ]**. Colle le lien, clique **🪄 Extraire**, puis ouvre la fenêtre dédiée.
- **Sondage** : onglet **🗳️ Sondages & Chat**. Écris la question et les choix ; les spectateurs votent en tapant `!1`, `!2`, `!3` ou `!4`.
- **Encart d'annonce** : onglet **🪧 Bannières**. Écris ton message, et **F4** l'affiche ou le masque.

### Placer et redimensionner les encarts
Directement sur la scène, ouverte dans le navigateur ou dans TikTok LIVE Studio :
- **glisse** un encart pour le déplacer ;
- tire ses **bords ou son coin** pour le redimensionner.

Le contenu suit automatiquement :
- l'annonce, les numéros d'urgence, l'avertissement et la question épinglée **grossissent ou rétrécissent** pour remplir le cadre, sans jamais déborder ;
- la grille des numéros passe sur 1, 2 ou 3 colonnes selon la largeur ;
- la vidéo occupe tout son encart.

La disposition est **enregistrée par Live Régie** : tu retrouves la même dans TikTok LIVE Studio et dans le navigateur, même après une relance.

Réglages dans **Préférences → 🖼️ Mise en page des scènes** :
- ajustement automatique activé ou non ;
- vidéo en « Cadre entier » ou en « Remplir » ;
- **↺ Réinitialiser la mise en page**, qui remet tous les encarts à leur place d'origine sur toutes les scènes.

### Si le chat se coupe
Pas besoin de toucher à quoi que ce soit : la régie **se reconnecte toute seule**. Le statut du chat indique « nouvelle tentative dans N s ».

---

## 6. Après le live

Onglet **Régie** → **📝 JOURNAL DU LIVE** → **🏁 Terminer le live & enregistrer le journal**.

Un fichier est créé dans le dossier `journaux/` (bouton **📂 Dossier**). Il contient :
- la durée et les chiffres : commentaires, cadeaux, partages, follows, demandes d'aide ;
- la chronologie : thèmes, chronos, bouclier, questions épinglées, sondages ;
- la liste des **demandes d'aide** de ce live, pour recontacter les personnes.

> 🔒 Ce fichier contient des messages de personnes vulnérables : il reste sur ton PC et n'est **pas** envoyé sur GitHub.

---

## 7. Raccourcis clavier

Ils fonctionnent même si la régie est réduite ou si tu es dans une autre fenêtre.

| Touche | Action |
| :--- | :--- |
| **F1** | Sirène (jingle alerte) |
| **F2** | Épingler / retirer la dernière question |
| **F3** | Afficher / masquer les numéros d'urgence |
| **F4** | Afficher / masquer l'encart d'annonce |
| **F8** | Bouclier anti-raid |
| **F9 / F10 / F11** | 1re / 2e / 3e branche |
| **F12** | Afficher / masquer la vidéo |

> ⚠️ Comme ils sont globaux, appuyer sur F1 dans une autre application déclenche aussi la sirène.

---

## 8. Personnaliser

### Les branches (thèmes du live)
Onglet **[ Préférences ]** → **🎛️ BRANCHES DU LIVE**. Ouvre une branche pour régler :
- le **nom du bouton** (avec emoji) et les **couleurs** ;
- l'étiquette et le titre affichés sur la scène, le sujet et le sous-titre par défaut ;
- le mode du bandeau, et l'affichage automatique des numéros d'urgence ;
- les **fonds d'écran** 9:16 et 16:9 (**📂 Choisir…**).

Tu peux aussi **➕ Ajouter** une branche, **⬆ Monter** pour changer l'ordre (et donc les raccourcis F9 à F11) ou **🗑 Supprimer**.

Rien n'est appliqué avant **💾 Enregistrer**. **Annuler** abandonne les modifications.

### Le bot du chat
Onglet **🗳️ Sondages & Chat** → **⚙️ Configurer le Bot…** : mots-clés d'urgence, mots interdits (filtre anti-insultes), commandes qui épinglent ou lancent un son.

### Les numéros d'urgence
Onglet **🪧 Bannières** → **⚙️ Configurer les Numéros d'Urgence…**.

### Les jingles
Onglet **🎵 Sons** → **➕ Ajouter Jingle**, en MP3 ou en vidéo.

---

## 9. En cas de souci

| Problème | Solution |
| :--- | :--- |
| Les scènes sont vides ou « introuvables » | Vérifie que Live Régie est ouverte, puis recharge la source dans TikTok LIVE Studio. |
| Le chat ne se connecte pas | Vérifie le pseudo (sans faute) et que tu es bien en live. « Module TikTokLive non installé » : lance `pip install TikTokLive`. |
| « Compte introuvable » | Le pseudo est mal orthographié : corrige-le et reconnecte. |
| Une question s'épingle toute seule | L'option « épingler automatiquement les questions » est activée dans la config du bot. |
| Un raccourci ne marche pas | Une autre application utilise peut-être la même touche. Ferme-la ou utilise le bouton dans la régie. |
| Je veux revenir aux thèmes d'origine | Préférences → Branches → **🔄 Rétablir les 4 branches d'origine** → **💾 Enregistrer**. |

---

## 10. Confidentialité

- Les scènes et la régie ne fonctionnent **que sur ton PC** (adresse `127.0.0.1`).
- Les **demandes d'aide** et le **journal** ne sont jamais affichés sur le live, ni envoyés sur GitHub.
- L'auto-tappeur (Préférences) automatise des tapotements : c'est **contraire aux règles de TikTok**, et ton compte peut être restreint si tu t'en sers.
