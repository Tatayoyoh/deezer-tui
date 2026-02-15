# Fanfare TUI

Bored to use 300M of RAM to play music ?

* for developers <3
* easy to use
* low memory footprint

Available :
* Deezer

Planned :
* background player, still playeing music
* login from browser/deezer.com link/backlink
* Add to favorites
* better shortcuts
* better UI
* Themes
* Spotify
* Youtube
* SoundCloud
* local music


## Other goods projects

https://github.com/ravachol/kew

https://tizonia.org/

https://musikcube.com/

https://github.com/timdubbins/tap

https://github.com/tramhao/termusic

https://www.kariliq.nl/siren/

https://github.com/raziman18/gomu

https://github.com/dhulihan/grump

https://github.com/Kingtous/RustPlayer



# fg (signal SIGTSTP/SIGCONT natif)
Quand tu fais Ctrl+Z, le shell envoie SIGTSTP au process. Le problème : SIGTSTP suspend TOUT le process, y compris les threads tokio et le thread audio rodio/cpal. La musique s'arrête. fg le reprend, mais il y aura un trou dans la lecture. Ce n'est pas ce que tu veux.

## Architecture client/serveur (daemon)
C'est la bonne approche pour ton cas :

1. deezer-tui démarre un daemon en arrière-plan (le player) qui tourne indépendamment du terminal
2. Le TUI est juste un client qui se connecte au daemon (via un socket Unix)
3. Ctrl+Z ou q ferme le TUI mais le daemon continue à jouer
4. Relancer deezer-tui se reconnecte au daemon existant

C'est exactement ce que font mpd+ncmpcpp, spotifyd+spt, etc.

**Mais** c'est un gros refactoring. Une solution intermédiaire plus simple :

##  Fork simple (recommandé pour l'instant)
1. Au Ctrl+Z, on restaure le terminal (quitte le mode raw/alternate screen)
2. Le process continue à tourner en arrière-plan (on intercepte SIGTSTP et on l'ignore)
3. La musique continue, l'auto-advance fonctionne via un thread dédié
4. Relancer deezer-tui détecte le process existant et lui envoie un signal pour réafficher le TUI

Ou encore plus simple : intercepter Ctrl+Z pour juste masquer le TUI (restaurer le terminal) tout en gardant le process actif, puis deezer-tui envoie SIGUSR1 au process existant pour le réafficher.

Qu'est-ce que tu préfères ?