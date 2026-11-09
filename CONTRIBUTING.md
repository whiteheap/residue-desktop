# Contributing to residue-ui

residue-ui is the residue Mac app. It moves worktrees to the trash through the residue CLI it bundles, so every change is judged by one question: could it make the app remove something a person would want back, or show something about a worktree that residue did not say? Bug reports and fixes are welcome.

## Report a bug

Open an issue with:

- the two versions at the bottom of the app's sidebar, residue's and the app's, your macOS version, and whether your Mac has Apple silicon or an Intel processor
- what you did, what you expected, and what happened
- for a worktree the app shows wrongly, the output of `residue explain --json PATH` (remove any paths or branch names you don't want public)

If the app removed a worktree that held work you wanted, or ran residue on anything other than what its confirmation showed, do not open a public issue. Report it privately, as [SECURITY.md](SECURITY.md) describes. A bug in what residue itself decides goes to [residue-cli](https://github.com/whiteheap/residue-cli/blob/main/CONTRIBUTING.md#report-a-bug).

## Propose a change

Open an issue before you start on anything larger than a small fix, so we can agree on the behavior first. [AGENTS.md](AGENTS.md#invariants) lists the invariants: the app never decides what is safe to remove, removal runs only the plan its confirmation showed, every child process starts through one function, and the app bundles exactly one residue-cli release. A change that weakens one of them needs a decision recorded in [CHANGELOG.md](CHANGELOG.md), so raise it in an issue first.

## Set up

You need macOS 13 or later with Xcode's command line tools (`xcode-select --install`), rustup, Node 22.12 or later, and pnpm, the version in `package.json`'s `packageManager`. rustup installs the Rust version in `rust-toolchain.toml`, with both macOS targets, on the first build. With [gh](https://cli.github.com) logged in, the build also checks the bundled residue's build-provenance attestation.

```sh
git clone https://github.com/whiteheap/residue-ui
cd residue-ui
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm tauri dev` places the bundled residue, then opens a debug build. To build the app as a release does:

```sh
pnpm tauri build --target universal-apple-darwin
```

The bundle is `src-tauri/target/universal-apple-darwin/release/bundle/macos/residue.app`, next to a DMG in `bundle/dmg/`. The DMG step arranges its window through Finder, which can time out from a terminal that macOS has not allowed to control Finder. With `CI=true`, as on GitHub Actions, it skips that step.

### The bundled residue

The app bundles the residue CLI of one residue-cli release. `sidecar.lock` names its version and the sha256 of its two macOS archives, as that release's `SHA256SUMS` lists them. `scripts/sidecar.sh` places it where Tauri's `externalBin` takes it, `src-tauri/binaries/residue-cli-<target>`, and `tauri dev` and `tauri build` run that script first. Tauri copies the binary into the bundle as `Contents/MacOS/residue-cli`, beside the app's own executable, `residue`. Nothing in `src-tauri/binaries/` is committed.

- `sh scripts/sidecar.sh` downloads the two archives, checks each one's sha256 against the lock and, when gh is logged in, its build-provenance attestation, and joins the two binaries with `lipo` into `residue-cli-universal-apple-darwin`. A release already placed is kept, so the next run downloads nothing. Without gh, it says that it did not check the attestation.
- `sh scripts/sidecar.sh --require-attestation` fails when gh cannot check the attestation. CI and the release run it.
- `sh scripts/sidecar.sh --check` changes nothing, and fails unless the pinned release is placed, with its attestation checked, and unchanged since. `scripts/check-app.sh` runs it.

Releases up to 0.0.10 were built before residue-cli was renamed, so their attestations name `whiteheap/residue`. `attested_as` in `sidecar.lock` names the repository to check.

To move to another release, set `version`, `attested_as` and both checksums in `sidecar.lock`, read that release's entry in [residue-cli's CHANGELOG](https://github.com/whiteheap/residue-cli/blob/main/CHANGELOG.md), and update what the app says about residue's behavior in the same pull request ([AGENTS.md](AGENTS.md#invariants)).

### A residue from your checkout

To try the app with a change to the CLI, build the bundled residue from a residue-cli checkout:

```sh
RESIDUE_CLI_SOURCE=../residue-cli pnpm tauri dev
```

`scripts/sidecar.sh` then runs `cargo build --locked --release` on that checkout for both architectures, with this repository's Rust toolchain, and the binaries go to the checkout's `target/`. `--check`, and so `scripts/check-app.sh`, CI and the release, refuse such a build. The next run without the variable places the pinned release again.

## Checks

CI, `.github/workflows/ci.yml`, runs these on every pull request and on `main`, whose ruleset requires it:

```sh
sh scripts/sidecar.sh --require-attestation
pnpm typecheck
pnpm lint
pnpm build
(cd src-tauri && cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings)
pnpm tauri build --ci --target universal-apple-darwin -- --locked
sh scripts/check-app.sh src-tauri/target/universal-apple-darwin/release/bundle/macos/residue.app
```

`pnpm build` also fails when the app uses a Phosphor icon weight the build drops, or bundles a Kumo component that `src/styles.css` does not scan (`bundle.ts`). `scripts/check-app.sh` checks the bundle: the identifier `com.whiteheap.residue`, the name `residue`, the version in `tauri.conf.json`, both architectures in the app and in its `residue-cli`, that `residue-cli` is the pinned release by its version (on Apple silicon with Rosetta, the x86_64 slice's too), its attestation check and each architecture's UUID, and the ad-hoc signature. CI keeps the DMG as the run's artifact, `residue-macos-dmg`.

The app has no automated test suite. A change is checked by these commands and by using the app on scratch repositories, as [Verify a change](#verify-a-change) says.

## Development builds

These environment variables change what a debug build (`pnpm tauri dev`) does. Release builds ignore them.

- `RESIDUE_APP_DEV_SERVICE=preview`: the Automatic cleanup switch never runs `residue service install` or `uninstall`. launchd acts on your real login session, and the service label is shared with any residue service already running, so a real install or uninstall would replace or remove it. In preview the switch shows the command it would run and the output of `residue watch --once --dry-run`, which removes nothing. Use it whenever you work on that screen on a machine that runs residue's service.
- `RESIDUE_APP_DEV_AGENT_FILE=PLIST`, with the preview on: the preview reads `PLIST` as the installed service's agent file, so you can see how the screen reads a service another residue version wrote. It reads the file again on every read, as it reads the real one, until you use the switch: change the file while the save dialog is open, and Save refuses and shows what changed. The app only reads the file. Make one in a temporary folder with `plutil -create xml1 PLIST` and `plutil -insert ProgramArguments -json '["/opt/homebrew/bin/residue","watch","--no-fetch","/tmp/x"]' PLIST`.
- `RESIDUE_APP_DEV_SNAPSHOT_DIR=DIR`: write a name into `DIR/request` and the app saves a picture of its window to `DIR/NAME.tiff`, for checking the UI from a terminal that macOS does not allow to record the screen. `sips -s format png DIR/NAME.tiff --out DIR/NAME.png` converts it. The picture is at twice the window's size. The README's screenshots came from it, with a 1180 by 888 point window and a scratch `HOME` whose example repositories are under `~/src`, so no real path or name shows. The UI is grayscale, so they are stored as 8-bit gray PNGs, which lose nothing.

## Verify a change

On a Mac that runs residue's service, its data and its service are not test fixtures:

- Never run `residue service install` or `uninstall`, also not with `--dry-run` or a scratch `HOME`. Use `RESIDUE_APP_DEV_SERVICE=preview` to exercise the switch. `residue service status` only reads.
- Remove and restore only in scratch repositories you create in a temporary folder. A scan of real folders only reads, and is fine for a realistic overview.

## State machines

The app keeps one state of its own with a lifecycle: the confirmed removal in flight, `InFlight` in `src-tauri/src/removal.rs` (#12).

- `None` to `Starting` on a confirm, before residue starts. A confirm in any other state is refused: that is how a second removal is refused.
- `Starting` to `Running` once residue runs and the system gives its start time, or to `None` when `remove` returns first.
- `Running` to `None` when `remove` returns, or to `Left` when its residue still runs then (the wait for it failed).
- `Left` to `None` when that process exits. An app that opens and finds a marker whose process still runs starts in `Left`.

`InFlight::next` decides every move, with no wildcard arm. `Removals::apply` makes each one under one lock and says it on stderr, or says why it is illegal and changes nothing. The marker, `removal-running.json` in the app's data directory, is the one effect outside the app: it is written whole once residue runs (to a `.part` file opened before residue starts, then renamed), removed on every move to `None`, and checked when the app reads it, at launch: one with another schema, or one that does not parse, names no removal and is removed.

Worktrees, trash entries and removal records have the states residue reports in its `--json` documents, read again before each action; their lifecycles are in [residue-cli's CONTRIBUTING.md](https://github.com/whiteheap/residue-cli/blob/main/CONTRIBUTING.md#state-machines). The automatic cleanup service is launchd's, read with `residue service status`. What else the app writes has no states: the folders it scans, in `settings.json` in its config directory (`src-tauri/src/settings.rs`), and the copy of residue a service runs when no residue you installed is new enough (`src-tauri/src/service.rs`, `copy_sidecar`). A save of the service compares before it writes: `install` reads the service again and installs nothing when the read differs from the one its dialog showed (`Basis`, `Save::Changed`), and it runs only while no other save or turn-off of the app runs (`Changes`). residue then compares once more, as one step with the write: `install` passes the fingerprint of the dialog's read as `residue service install --expect`, and residue writes nothing and exits 4 when the service no longer has it (#11).

## Performance

The window stays open beside your editor, your builds and your coding agents, so the CPU and memory it spends are yours. A pull request that touches a hot path, or adds a dependency, a thread, a child process or a timer, gives in its description every run of both builds, the Mac, how you checked that the window shows the same, and what you did not measure. A change that makes a number worse says why it is worth it.

### Measure a change

Compare release builds (`pnpm tauri build`) on one Mac, both builds interleaved, three runs or more each, each launched with a scratch `HOME` that holds the same example repositories:

- memory: `footprint -p PID` of the app (`residue`), its WebContent and its GPU process, after the same task, such as a scan of the same folders;
- CPU: `ps -o utime=,stime= -p PID` of each, before and after that task, so system CPU stays apart from user CPU;
- size: the DMG's bytes, and the bytes of `dist/assets/*.js` and `dist/assets/*.css` after `pnpm build`.

### Main's numbers

Each comes from the pull request that last changed it, on release builds with a scratch `HOME` on one Mac (an Apple M4 Max).

- One repository with 300 linked worktrees, after a scan: the app 34 to 35 MB, WebContent 113 to 117 MB, GPU 17 to 18 MB. A rescan costs WebContent 0.12 to 0.17 s of CPU (residue-cli#90).
- 20 s of pointer movement over the Worktrees map: WebContent 1.88 to 2.19 s of CPU (residue-cli#90). Along a worktree's bar, with 150 worktrees: WebContent 1.00 s user and 0.22 s system CPU (medians), and, in a debug build with counters, 87 to 96 renders of the bar (residue-cli#113).
- The JavaScript is 566,676 B and the stylesheet 94,994 B (`pnpm build`, residue-cli#126, residue-cli#130 and this repository's #4, #14, #12, #17 and #13).

Found something slow, in the app, a library or a tool it runs? Open an issue with the measurement and the command that repeats it, before there is a fix.

## Open a pull request

1. Branch from `main` and keep one change per pull request.
2. Start each commit message with a type such as `feat:`, `fix:` or `docs:`, as the history does.
3. For a change users will notice, add a line to `CHANGELOG.md` under `## Unreleased`, at the top. Create the heading if it isn't there.
4. A change that alters a screen takes that screen's README screenshot again ([Development builds](#development-builds)).
5. In the description, say which macOS and Mac you ran it on and what you did in the app, and list the commands you ran and the ones you did not.
6. For a hot path, a dependency, a thread, a child process or a timer, add every run from [Performance](#performance).
7. CI must pass.

## Releases

A maintainer releases the app by tagging a version, as [AGENTS.md](AGENTS.md#release) describes. Each release publishes `residue-macos.dmg` with `SHA256SUMS` and a build-provenance attestation, and its notes name the residue-cli release inside.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
