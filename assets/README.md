# Generate GIF assets

VHS project is used to generate assets

https://github.com/charmbracelet/vhs

```bash
cd assets
vhs search.tape
```

## Overlay feature

```bash
cd assets
git clone https://github.com/pkazmier/vhs.git
cd vhs
git switch captino-overlay
go build
cd ..
./vhs/vhs search.tape
```

pull request :
https://github.com/charmbracelet/vhs/pull/719