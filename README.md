# residue desktop

A desktop app for [residue](https://github.com/whiteheap/residue): see the storage that Git worktrees take, and reclaim it, in a window instead of a terminal.

Work in progress. The app is a window over the residue CLI and never reimplements a check, a size or a deletion: every action runs the bundled `residue` binary and reads its JSON.

## Build

Needs Rust (the version in `rust-toolchain.toml`), Node 22.12 or later, pnpm, and an authenticated `gh` (it verifies the sidecar's provenance attestation).

```sh
pnpm install --frozen-lockfile
sh scripts/fetch-sidecar.sh          # residue from sidecar.lock, verified
pnpm tauri dev
pnpm tauri build --target universal-apple-darwin
```

## License

MIT
