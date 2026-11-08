# The residue Mac app

This repository holds the residue Mac app: a window over the residue CLI that lists the linked worktrees under your code folders, moves the ones that can go to the trash, restores them from the trash or from a removal record, shows what residue has freed, and turns automatic cleanup on or off. What it does for users, the download and the first launch are in [residue-cli's README](https://github.com/whiteheap/residue-cli#mac-app); screenshots: [Worktrees](https://github.com/whiteheap/residue-cli/blob/main/docs/screenshots/worktrees.png), [Freed](https://github.com/whiteheap/residue-cli/blob/main/docs/screenshots/freed.png) and [Restore](https://github.com/whiteheap/residue-cli/blob/main/docs/screenshots/restore.png).

The app never reimplements a check, a size or a deletion. Every action runs the residue binary bundled inside the app, with explicit arguments, and reads its JSON. That binary is the residue-cli release that `sidecar.lock` pins, checked by its sha256 and its build-provenance attestation. It is Tauri 2: Rust in `src-tauri/`, React and TypeScript in `src/` (Kumo components, Phosphor icons, Tailwind 4). [AGENTS.md](AGENTS.md) has the invariants.

The app's whole history is here: its first commits, then those made in residue-cli's `app/` folder, merged in 04955a5.

## Build from source

[CONTRIBUTING.md](CONTRIBUTING.md) has the setup, the bundled residue, the checks and the development builds.

## Security

[SECURITY.md](SECURITY.md) says how to report a security issue privately.

## License

MIT, in [LICENSE](LICENSE).
