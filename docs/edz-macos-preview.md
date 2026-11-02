# LightCraft EdZ Lab — essai macOS Intel

Version personnelle pour macOS Ventura 13 ou ultérieur, processeur Intel.
La base du code est LightCraft 0.17.0, avec les éditeurs externes de la branche
`feature/affinity-nik`. Ce n'est pas une version officielle.

## Installation

Télécharger l'artefact GitHub Actions, décompresser puis décompresser le ZIP
`LightCraft-EdZ-Lab-macOS-Intel.zip` qu'il contient. Glisser **LightCraft EdZ Lab.app**
dans Applications. Conserver LightCraft actuel.

Cette version est signée localement, sans certificat Apple ni notarisation.
Si macOS bloque son ouverture, utiliser Réglages Système → Confidentialité et
sécurité → Ouvrir quand même, uniquement après avoir vérifié l'origine du fichier.
Ne pas désactiver globalement la protection de macOS.

## Isolation et premier essai

Les préférences et la photothèque de test sont dans
`~/Library/Application Support/LightCraft EdZ Lab`.
Lancer l'application par son icône, pas le binaire interne. Ne pas ouvrir la
photothèque de LightCraft 0.9.1 avec cette version : la compatibilité inverse
n'est pas garantie. Importer seulement des **copies de photos** pour commencer.
Les mises à jour officielles sont bloquées par un fichier de version propre
à ce paquet afin de conserver cette isolation.

Dans les paramètres généraux, rubrique édition externe, ajouter Affinity Photo 2
puis Silver Efex Pro 2 et les autres applications Nik souhaitées. Choisir leurs
fichiers `.app`, TIFF, 16 bits, Adobe RGB pour le premier essai.

1. Développer une copie de photo dans LightCraft EdZ Lab.
2. Choisir Photo → Modifier dans → Affinity Photo 2.
3. Modifier l'image dans Affinity puis enregistrer **dans le même TIFF**,
   en conservant le format TIFF et 16 bits (pas un fichier `.afphoto`).
4. Revenir à LightCraft et vérifier l'image actualisée ainsi que la pile.
5. Recommencer avec Silver Efex Pro 2, appliquer puis enregistrer le même TIFF.

La validation réelle de l'enregistrement et du retour de Google Nik Collection
1.2.11 reste à effectuer sur l'iMac. L'exécution du compilateur sur un Mac récent
ne garantit pas à elle seule le fonctionnement sous Ventura.

## Recompiler

Sur un Mac disposant des outils de développement Xcode et de Rust 1.95 ou plus,
à la racine du dépôt : `bash packaging/edz/build-macos.sh`.
Le script télécharge les polices à la révision épinglée du projet et crée le ZIP
dans `dist/release`. Il ne publie rien et ne nécessite aucun secret Apple.
