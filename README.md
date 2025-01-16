# Simple local HTTP server for crates.io-index

## Usage

### Set up config.toml
```toml
[repo]
path = "./crates.io-index"
git_url = "https://github.com/rust-lang/crates.io-index.git"
update_interval = "60" 

[web]
address = "127.0.0.1"
port = 8000
```

### Run server
```bash
cargo run --release
```

### Set up ~/.cargo/config.toml
```toml
[source.crates-io]
replace-with = 'local'

[source.local]
registry = "sparse+http://127.0.0.1:8000/"
```

Done.