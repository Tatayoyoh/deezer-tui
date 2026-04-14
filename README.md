# Deezer TUI

[![CircleCI](https://dl.circleci.com/status-badge/img/gh/Tatayoyoh/deezer-tui/tree/main.svg?style=shield)](https://dl.circleci.com/status-badge/redirect/gh/Tatayoyoh/deezer-tui/tree/main)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS%20%7C%20WSL2-blue.svg)]()
[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)
[![Claude](https://img.shields.io/badge/Claude-D97757?logo=claude&logoColor=fff)](#)

![text](assets/logo.png)

Bored to use 300M of RAM to play music ?

* for developers <3
* easy account login
* low memory footprint
* music playing in the background
* compliant with Deezer features

## Install

Linux / macOS (one-liner)
```bash
curl -LsSf https://raw.githubusercontent.com/Tatayoyoh/deezer-tui/main/install.sh | sh
```

Or copy binary yourself
```bash
wget -qO deezer-tui "https://github.com/Tatayoyoh/deezer-tui/releases/latest/download/deezer-tui-linux-x86_64"
chmod +x deezer-tui
sudo mv deezer-tui /usr/local/bin/deezer-tui
```

## Features

✅ Login through deezer.com<br>
✅ Background player with [ctrl+z]<br>
✅ Deezer Flow [f]<br>
✅ Search / favorites / radios pages<br>
✅ Playing track context menu [ctrl+space]<br>
✅ Focused element context menu [x]<br>
✅ Album page [a]<br>
✅ Artist page [t]<br>
✅ Waiting list [w]<br>
✅ Shortcut menu [?]<br>
✅ Global app menu [ctrl+o] <br>
✅ Themes, from official Deezer themes<br>
✅ Translations <br>
✅ Offline mode with downloaded tracks<br>
✅ Album/Artist miniature (require Kitty or Ghostty for real image display)<br>
✅ Auto update

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

[Caveman](https://github.com/juliusbrussee/caveman) is used to compress CLAUDE.md prompt, saving some session tokens.

## Other goods projects

Deezer players
* https://github.com/aunetx/deezer-linux - Deezer desktop app packaged into a webview
* https://github.com/yne/dzr - Deezer music from command line
* https://github.com/Minuga-RC/deezer-tui - another good TUI for Deezer 
* https://gitlab.com/ColinDuquesnoy/MellowPlayer - Deezer desktop app packaged into a webview

Terminal audio players
* https://github.com/tramhao/termusic
* https://musikcube.com/
* https://github.com/timdubbins/tap
* https://github.com/ravachol/kew
* https://www.kariliq.nl/siren/
* https://github.com/raziman18/gomu
* https://github.com/dhulihan/grump
* https://github.com/Kingtous/RustPlayer


