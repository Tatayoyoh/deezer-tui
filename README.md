# Deezer TUI

Bored to use 300M of RAM to play music ?

* for developers <3
* easy account login
* low memory footprint
* compliant with deezer features

## Install

Linux / Mac
```bash
wget -qO deezer-tui "https://github.com/Tatayoyoh/deezer-tui/releases/latest/download/deezer-tui-linux-x86_64"
chmod +x deezer-tui
sudo mv deezer-tui /usr/local/bin/deezer-tui
```

## Features

✅ background player, still playeing music<br>
✅ login from browser/deezer.com link/backlink<br>
✅ search and favorites pages<br>
✅ playing track context menu [ctrl+p]
✅ focused track context menu [m]
⬜ Linux Gnome desktop integration (next, pause back, open deezer-tui, quit deezer-tui) <br>
✅ Album page (or popup) when entering an album <br>
⬜ Artist page (or popup) when entering an artiste <br>
⬜ Track/album Miniature https://ratatui.rs/showcase/apps/#eilmeldung <br>
✅ Display waiting list as modal <br>
✅ shortcut modal <br>
⬜ better UI <br>
⬜ better shortcuts <br>
✅ Contiuous Integration <br>
✅ global app menu [ctrl+o]
✅ Themes <br>
⬜ Displayed sections <br>
⬜ More app parameters <br>


## Build on your system

First install
```bash
sudo apt install pkg-config
sudo apt install libasound2-dev
curl https://sh.rustup.rs -sSf | sh
source ~/.bashrc
```

Build
```bash
cargo build --release
```

## Other goods projects

* https://github.com/yne/dzr
* https://github.com/ravachol/kew
* https://tizonia.org/
* https://musikcube.com/
* https://github.com/timdubbins/tap
* https://github.com/tramhao/termusic
* https://www.kariliq.nl/siren/
* https://github.com/raziman18/gomu
* https://github.com/dhulihan/grump
* https://github.com/Kingtous/RustPlayer


