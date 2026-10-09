# Install

TPT App Policy ships as a single binary, `tpt-policy`. Platforms at launch:
Windows x64 and Linux x64.

> Licence: dual MIT or Apache-2.0, at your option. See `LICENSE-MIT` and
> `LICENSE-APACHE`.

## Build from source

You need a Rust toolchain. The version is pinned in `rust-toolchain.toml`.
Install it with [rustup](https://rustup.rs).

```sh
git clone https://github.com/TPT-Solutions/tpt-app-policy
cd tpt-app-policy
cargo build --release
```

The binary is at:

- Windows: `target\release\tpt-policy.exe`
- Linux: `target/release/tpt-policy`

Copy it somewhere on your `PATH`, or run it by its full path.

## Check the install

```sh
tpt-policy --version
tpt-policy doctor
```

`doctor` checks the version, the platform, that the working directory is
writable, and runs a built-in self-test. It exits `0` when every required check
passes and `5` when one fails. The `./tpt` config folder is optional.

## Release bundle (when published)

```sh
scripts/package.sh
```

This writes `dist/tpt-app-policy-<version>-<platform>.tar.gz` and
`dist/SHA256SUMS`. Verify the archive before you unpack it:

```sh
sha256sum -c SHA256SUMS
```

The script refuses to build if either licence file is missing. Both licence
files are copied into the bundle.

## Docker (not yet tested)

```sh
docker build -t tpt/app-policy .
```

The `Dockerfile` is written but has not been built or run. See
[INTEGRATION.md](INTEGRATION.md) for the run commands once it is tested.

## Offline use

The binary makes no network calls. Installing needs the network only for the
Rust toolchain and crate downloads during the build.
