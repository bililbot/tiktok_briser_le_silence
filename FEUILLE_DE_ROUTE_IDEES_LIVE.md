# 📜 FEUILLE DE ROUTE & IDÉES D'ÉVOLUTION — LIVE TIKTOK « BRISER LE SILENCE »

Ce document répertorie les concepts et fonctionnalités validés à implémenter lors des prochaines sessions de développement.

---

### 🛡️ 1. Bouton Panique / « Mode Bouclier Anti-Raid »
* **Objectif** : Protéger le streamer et le compte TikTok d'un ban algorithmique en cas d'attaque de trolls ou de dérapage dans le chat.
* **Comportement en 1 clic** :
  * Masque instantanément le chat à l'écran.
  * Affiche un écran de transition propre : *« Pause Modération — Débat en cours de régulation »*.
  * Coupe le micro ou enclenche une musique d'intermède neutre.

---

### ⏱️ 2. Chronomètre & Gestion du Temps de Parole (Mode Débat)
* **Objectif** : Rythmer les interventions et éviter les monopoles de parole.
* **Fonctions** :
  * Présélections en 1 clic : 1 min, 3 min, 5 min.
  * Jauge circulaire ou barre de progression discrète sur la scène.
  * Alerte visuelle (orange puis rouge) à 15 secondes de la fin.
  * Gong ou jingle sonore discret automatique à la fin du temps.

---

### 🌹 3. Pop-up Automatique de Remerciement (Cadeaux & Partages)
* **Objectif** : Valoriser les spectateurs et stimuler l'algorithme sans interrompre la parole du streamer.
* **Fonctions** :
  * Détection des événements cadeaux (`GiftEvent`) et partages (`ShareEvent`).
  * Micro-bulle animée de 3 secondes en bas à droite : *« 💜 Merci @pseudo pour la Rose ! »*.

---

### 📊 4. Jauge d'Objectif du Live (Goal Bar)
* **Objectif** : Créer un objectif collectif pour les spectateurs (Likes / Partages).
* **Fonctions** :
  * Barre néon animée (ex : *« Objectif 10 000 J'aime pour le prochain sujet »*).
  * Remplissage automatique ou curseur manuel depuis la télécommande.

---

### ⌨️ 5. Raccourcis Clavier Globaux (Stream Deck Virtuel)
* **Objectif** : Tout contrôler sans toucher à la souris pendant le Live.
* **Exemples d'assignation** :
  * `F1` : Jingle Alerte / Sirène
  * `F2` : Épingler la dernière question du chat
  * `F3` : Afficher / Masquer les numéros d'urgence
  * `F4` : Afficher / Masquer l'encart d'annonce libre
  * `Espace` : Play / Pause Musique

---

### 📺 6. Ticker Défilant « Breaking News / Messages Permanents »
* Bandeau défilant fluide en bas d'écran avec chiffres-clés (ex: *119 Enfance en danger, 3919 Violences*, rappels de prévention, citations).
