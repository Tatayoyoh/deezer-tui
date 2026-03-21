# Deezer TUI

[![CircleCI](https://dl.circleci.com/status-badge/img/gh/Tatayoyoh/deezer-tui/tree/main.svg?style=shield)](https://dl.circleci.com/status-badge/redirect/gh/Tatayoyoh/deezer-tui/tree/main)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS%20%7C%20WSL2-lightgrey.svg)]()

![text](assets/logo.png)

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
✅ search / favorites / radios pages<br>
✅ playing track context menu [ctrl+p]<br>
✅ focused track context menu [m]<br>
⬜ Linux Gnome desktop integration (next, pause back, open deezer-tui, quit deezer-tui) <br>
✅ Album page when entering an album or with [a] shortcut<br>
⬜ Artist page (or popup) when entering an artiste <br>
⬜ Track/album Miniature https://ratatui.rs/showcase/apps/#eilmeldung <br>
✅ Display waiting list as modal <br>
✅ shortcut modal <br>
🔄 better UI <br>
🔄 better shortcuts <br>
✅ Contiuous Integration <br>
✅ global app menu [ctrl+o] <br>
✅ Themes, from official Deezer themes<br>
✅ Translations <br>
⬜ Offline mode <br>

![themes](assets/themes.gif)

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

## Made with our brave Claude Code

And drived by human goods ideas.

To be honest, I am not a Rust developer :p. Rust was a good match for this project 👍.

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


