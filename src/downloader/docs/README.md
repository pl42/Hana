# <h1 align="center"> 🧬 Hana 🧬 </h1>

Next-generation implementation of Ethereum protocol ("client") written in Rust, based on [Erigon architecture](https://github.com/ledgerwatch/interfaces).

## Why run Hana?

Look at Mgas/s.

![](./src/res/readme-screenshot.png)


## Building the source

Install `rustup` from rustup.rs.

```
git clone https://github.com/PL42/Hana
cd Hana
cargo build --all --profile=production
```

You can find built binaries in `target/production` folder.

## Running

* `Hana` takes an _already synced_ [Erigon](https://github.com/ledgerwatch/erigon) database with downloaded blocks and headers (stages 1-3), imports them, executes and verifies state root:

```
hana --datadir=<path to Hana database directory> --erigon-datadir=<path to Erigon database directory>
```

* `hana-toolbox` provides various helper commands to check and manipulate Hana's database. Please consult its help for more info:
```
hana-toolbox --help
```
