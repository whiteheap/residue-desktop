---
name: verify-confirm-flows
description: Model-check every order of your clicks, residue's replies and changes outside the app in the two flows where the app moves or deletes something, moving worktrees to the trash and turning automatic cleanup on, saving its folders or turning it off, to show whether any order moves a worktree the confirm did not list or installs a service the dialog did not show. Use before you merge a change to src/views/RemoveDialog.tsx, the removal dialog's wiring, `watchRemoval` or `go` in src/App.tsx, `changingService` in src/lib/api.ts, `chosen`, the checkboxes or the move buttons in src/views/Worktrees.tsx and src/views/Detail.tsx, `remove`, `plan_removal`, `remove_to_trash`, `removal_running` or `wait_for_removal` in src-tauri/src/lib.rs, src-tauri/src/removal.rs, `Basis`, `read`, `plan`, `install`, `uninstall` or `Changes` in src-tauri/src/service.rs, or the save and turn-off dialogs in src/views/Automatic.tsx, and when someone asks whether a click can move or delete more than the app showed.
---

# Verify the confirm flows

The app moves worktrees to the trash in two runs of residue: a plan that moves nothing, then, after your click, the same command with `--yes` and the planned paths. It saves automatic cleanup from a dialog that reads the service, shows it, and sends that read back, so that `install` installs nothing when the service has changed, and residue, given that read's fingerprint, writes nothing when it changed after that. Between those steps you can click, residue can reply late or fail, the window can start afresh or the app quit and open again, and a terminal, `brew upgrade` or a new app put in place of this one can change the service or the residue a save installs. This check tries every order of those steps, for two worktrees and for two folders, and says which of fifteen rules any order breaks.

It is two TLA+ models of the app, `model/RemoveFlow.tla` and `model/SaveFlow.tla`, checked with TLC. TLC does not read the TypeScript or the Rust: a model holds only while it describes the code, which is why the coverage table and the sync commit below exist.

## Run it

```sh
bash .agents/skills/verify-confirm-flows/check.sh
```

It takes about seven minutes and exits 0 when the result matches `model/expected`: every rule holds except the known breaks it lists, each with its shortest path, and every mutant is caught. It exits 1 when the result differs, and 2 when the tools cannot be set up or TLC fails.

The first run downloads TLC and a Java runtime into `~/.cache/residue-tla` (or `$RESIDUE_TLA_TOOLS`), the cache residue-cli's model check uses, and checks each against its pinned SHA-256. It needs bash, curl, tar, and `shasum` or `sha256sum`. To use your own, set `TLA2TOOLS` to a tla2tools.jar and `JAVA` to a java binary (11 or later); the run says when either is not the pinned one.

| Tool | Version | SHA-256 |
| --- | --- | --- |
| [tla2tools.jar](https://github.com/tlaplus/tlaplus/releases/tag/v1.7.4) | v1.7.4 (TLC 2.19) | `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88` |
| [Temurin JRE](https://github.com/adoptium/temurin25-binaries/releases/tag/jdk-25.0.5%2B8), macOS arm64 | 25.0.5+8 | `72ebd76b502c849e88dea39afae51baf5f82f2b5b1b6389c28ea707b20451bce` |
| Temurin JRE, macOS x64 | 25.0.5+8 | `806902c796c670969b7b27df73b8f0fe0924a61bd65083aa53064115e49cab2d` |
| Temurin JRE, Linux x64 | 25.0.5+8 | `6fac98f0c8202efc3135a9e9e42aa61963ec42d1c5cbb40caf231e7b14a24ef6` |
| Temurin JRE, Linux arm64 | 25.0.5+8 | `963931723891302ebb2687178532c3d793d71e2577cd058061c7efb396140a86` |

`.github/workflows/model.yml` runs it on Linux when a pull request changes this skill. It is not a required check. It does not run for a change to the app alone, because TLC would check the same model and pass: the check catches a bug in a code change only once that change updates the model.

## The rules

Each rule is an invariant. TLC checks it in every state every order reaches.

Moving to the trash, `model/RemoveFlow.tla`:

| Rule | What it says | The harm when it breaks |
| --- | --- | --- |
| `OnlyListedWorktreesMove` | A worktree goes to the trash only when the list you confirmed showed it. | A worktree you never saw in the dialog is in the trash. |
| `ConfirmSendsTheList` | A confirm sends residue exactly the worktrees its list showed (AGENTS.md: "only that click runs it again with `--yes` and exactly the planned paths"). | residue gets paths you did not confirm, and only its own check stands between them and the trash. |
| `OpensOnlyForCanGo` | The dialog opens only for worktrees the window showed as can go, where you clicked. | You are asked to move a worktree the window said residue keeps. |
| `ListIsForYourChoice` | While the dialog lists or moves, its list is residue's plan for the worktrees you chose. | The dialog lists one choice and confirms another. |
| `NothingMovedIsTrue` | When the dialog says nothing moved, nothing it started moved. | You are told nothing moved while a worktree is in the trash. |
| `OneRemovalAtATime` | At most one confirmed run of `residue remove --trash --yes` runs at a time. | Two runs work on the same worktrees at once, a case residue-cli's own model does not cover. |

Automatic cleanup, `model/SaveFlow.tla`. `last` is the app's last install:

| Rule | What it says | The harm when it breaks |
| --- | --- | --- |
| `SaveIsWhatTheDialogShowed` | A save installs the folders and the mode you chose, with the options the dialog showed and the residue it named. | The service runs with options or a residue you were not shown. |
| `ReplacesOnlyWhatItShowed` | A save replaces only the service the dialog showed (AGENTS.md: "A save installs what its dialog showed or nothing"). | A service set up in a terminal since the dialog read it is replaced, unseen. |
| `NeverDropsAnArgument` | A save never drops an argument the app cannot read (AGENTS.md: "Never drop an option on a save"). | An option of a newer residue is gone from the service. |
| `FailedSaveKeepsTheResidue` | A save that installs nothing leaves the app's own copy of residue, which the service runs, as it was before the save. | A save residue refused still changes which residue the service runs, while it keeps the earlier folders and mode. |
| `WarnedBeforeItDeletes` | Every install comes from a dialog that said the service deletes instead of using the trash. | The service deletes worktrees for good and you were never told. |
| `PreviewIsOfTheTickedFolders` | The preview the dialog shows ran on exactly the folders it shows ticked (#9). | You save folders whose preview you never saw, beside one of other folders. |
| `SaveIsOfTheTickedFolders` | While the dialog saves, it shows ticked exactly the folders the save installs (#9). | The service watches folders the dialog did not show when you saved. |
| `TheDialogStaysWhileItSaves` | A save or turn-off keeps its dialog open until its reply arrives (#10). | Its result, or its error, shows nowhere, and a new dialog can start a second save. |
| `OneChangeAtATime` | At most one `install` or `uninstall` of the app runs at a time (#10). | Two `residue service install` runs race, and the one that ends last wins, whatever the window shows. |

`TypeOK`, in each model, checks that every variable keeps its type.

## What it checks, and the bounds

Moving to the trash: two worktrees, each one residue would move now (can go) or keep, and these steps, each one action of the model:

- You: tick a worktree, click "Move N to the trash..." on the action bar, open a worktree's checks and click "Move this worktree to the trash...", scan again, confirm, or close the dialog.
- residue: the plan lands or fails; the confirmed run moves what can still go, then its report lands or the run fails.
- src-tauri (`removal.rs`, #12): it refuses a confirm while a confirmed run is in flight, also one an earlier launch of the app left running. It answers the window's ask whether one is, made when the window starts and when the dialog closes, and the window's wait for it to end. The ask or the wait can fail.
- Outside the app: a worktree turns from can go to keep or back (a commit, a branch that lands), twice at most.
- Faults, at most one: the window starts afresh while residue runs, from the last-resort "Try again" in `src/main.tsx`, which keeps src-tauri's state, or from a quit and a relaunch, which starts it anew and reads the marker; or the dialog fails to draw.

Automatic cleanup: two folders, the app scanning one of them, a service that starts off or on with options set, and these steps:

- You: turn the switch on or off, click "Change folders", tick or untick a folder or add one, click Preview, save with the folders ticked and either mode, confirm turning it off, cancel, refresh the screen, or go to another screen and back (Command-1 to 5 or the sidebar).
- The app and residue: the dialog's own read, the preview and its reply, `Changes`, which refuses an `install` or `uninstall` while another runs, `install`'s read and its comparison with the basis the dialog sent, the blocked check, in `save/copy` the copy of the bundled residue a save writes as the app's own and its check of that copy's version, `residue service install --expect`, which writes only while the service still has the fingerprint of the dialog's read, and its reply, and `residue service uninstall` and its reply. Each reply can fail, also after it wrote the service. A save that writes nothing puts the app's copy back as it was.
- Outside the app, twice at most: a terminal turns the service on or off, changes its folders, its mode or its options, or adds an argument the app cannot read, or `brew upgrade` puts a newer residue where a save installs from. In `save/copy`, a new app, newer or older, is put in place of this one while it runs, so the bundled residue is another release. In `save/during` and `save/copy` these can also happen while a save or turn-off runs, between `install`'s read and residue's write included (#11).
- Faults, at most one, in `save/reload`: the window starts afresh while a save or turn-off runs on in src-tauri (the page loads again, or the last-resort "Try again"). A quit and a relaunch is not modeled here: it starts a new `Changes` (Not covered).

| Setup | Model | Faults | States | Distinct states |
| --- | --- | --- | --- | --- |
| `remove/no-faults` | RemoveFlow | none | 171,405 | 46,464 |
| `remove/reload` | RemoveFlow | the window starts afresh, or the app quits and opens again | 2,758,623 | 521,382 |
| `remove/draw` | RemoveFlow | the dialog fails to draw | 1,335,889 | 274,416 |
| `save/outside` | SaveFlow | none; every outside change, while no save or turn-off runs | 9,923,810 | 3,178,114 |
| `save/during` | SaveFlow | none; every outside change, also while a save or turn-off runs (#11) | 16,747,760 | 5,306,018 |
| `save/reload` | SaveFlow | the window starts afresh while a save or turn-off runs; one folder, no outside change | 77,392 | 24,243 |
| `save/copy` | SaveFlow | none; a save with the app's own copy, and a new app put in place of this one or a terminal turning the service on or off or switching its mode, also while a save runs; one folder | 247,419 | 83,516 |

Each setup opens a dialog at most twice. Every rule holds in every setup, so each count is of one run with every rule. When a rule breaks, TLC stops at its first break and reports the shortest path, and check.sh checks the other rules again without it: a rule that breaks in a setup is not checked further there, and a new way to break it shows only if it is shorter.

Assumptions:

- Each click, read and reply is one indivisible step: React runs a handler and its state updates whole.
- A reply lands in the dialog that asked for it, and only while that dialog is open: RemoveDialog's `live` flag, and the latest request in `useCli`. A removal's report still reaches App's `onRemoved` after its dialog closed.
- Each dialog is modal, and App ignores Command keys while the removal dialog is open (`if (removing) return`). The `NotModal` mutant shows what that guard keeps. Command-1 to 5 still reach `go` while the save dialog is open; `go` refuses another screen while a save or turn-off runs (`changingService`), and `LeaveWhileSaving` shows what that keeps.
- "Add a folder…" opens the folder picker as a sheet on the window (tauri-plugin-dialog sets the window as its parent; rfd then calls `beginSheetModalForWindow`), which takes no other click while it is open, so adding a folder is one step.
- residue checks every path again on each run and moves only what can still go. The model trusts it; residue-cli's `verify-removal-races` checks residue.
- A confirmed run replies after residue exited: `cli.rs` waits for it, and at the deadline stops it and waits. When that wait fails, residue may still run, and src-tauri keeps the run in flight until its process ends (`removal.rs`, `Left`).
- src-tauri's answer to the window's ask is read when it lands. While the window asks, Move to the trash is off, so no run starts in between; a run that ends in between can only shorten the wait.
- Outside `save/during`, a terminal or brew changes the service only while no `install` or `uninstall` call of the app runs.
- residue's comparison for `--expect` and its write are one step (`InstallLands`): both run under an exclusive lock on the folder of the agent file, which every `residue service install` and `uninstall` from 0.0.12 takes, so a terminal's install waits for the app's. `brew upgrade` takes no lock, but it changes neither the release of the residue that runs the install, the one the fingerprint names, nor what that residue writes; an upgrade after the write is the next change outside the app.
- In `save/copy`, every save installs with the app's own copy, and the service runs that copy. A residue the user installs that is at least as new as the bundled one makes a save use it instead (`find_installer`); `save/outside` and `save/during` check that kind, and `Basis.binary` refuses a save whose read switched from one kind to the other.
- The fingerprint names the release of the residue that read it: at the dialog's read, the bundled one (`installs_as`), and at the write, the one that runs `service install`, with the app's copy that copy.
- One window, and one copy of the app at a time.

## When a change touches the verified code

Run the check for a change to any function in this table, or for one that adds a way to open either dialog, to send residue a path, or to write or remove the service.

| Model actions | Code |
| --- | --- |
| `YouTick`, `Chosen`, `YouClickMove`, `MoveOn` | `Worktrees.tsx`: the row checkbox, shown only for a candidate, select-all, `chosen` (the ticked paths the shown scan calls candidates), the action bar's `onRemove(chosen)`, `disabled` unless `inFlight` is "none" |
| `YouOpenChecks`, `ChecksLand`, `YouLeaveChecks`, `YouClickMoveInChecks` | `Detail.tsx`: the checks (`residue explain`) and "Move this worktree to the trash...", shown only for verdict `candidate` and `disabled` unless `inFlight` is "none" |
| `YouRescan`, `ScanLands` | `App.tsx` `rescan`, Command-R; `useCli` |
| `Open` | `App.tsx` `setRemoving`, which mounts `RemoveDialog` |
| `PlanLands`, `PlanFails` | `RemoveDialog.tsx` the effect on `paths` with `live`; `lib.rs` `plan_removal`, `remove(..., confirmed = None)`; `ready`, the results `skipped` with verdict `candidate` |
| `YouConfirm`, `InFlight` | `RemoveDialog.tsx` `confirm`, which sends `ready` and the plan's `generated_at`; the confirm button, `disabled={running}`; `lib.rs` `remove`, which calls `s.removals.start()` before residue starts; `removal.rs` `Removals::start`, refused with kind `invalid` while a removal is in flight, and `InFlight::next`; the dialog's text for kind `invalid` |
| `RunMoves`, `RunLands`, `RunFails` | `lib.rs` `remove_to_trash`, `remove(..., confirmed = Some(AsOf))`, the one call that adds `--yes`, with `--as-of` the plan's `generated_at` (`AsOf::of_plan`, which refuses with kind `invalid` and runs nothing when the confirm sends no such time); `cli.rs` `Run::output_with` and its deadline; `removal.rs` `Removing::record`, which writes the marker, and `Removing`'s drop on return; `confirm`'s `.then` and `.catch`; `App.tsx` `onRemoved` |
| `YouClose` | `RemoveDialog.tsx` `onOpenChange` and `busy`; `App.tsx` the dialog's `onClose`, which calls `watchRemoval` |
| `HeardLands`, `RemovalEnds`, `HeardFails` | `App.tsx` `watchRemoval`, `asks`, `heard`, and the effect that calls `api.removalRunning()`, then `api.waitForRemoval()`, `rescan` and `reloadStats`, with its `.catch`; `lib.rs` `removal_running`, `wait_for_removal`; `removal.rs` `Removals::running`, `wait`, `refresh` and `Process::runs` |
| `DrawFails`, `YouCloseBrokenDialog` | `App.tsx` the removal dialog's `ErrorBoundary` and its `onClose`, which scans again and calls `watchRemoval` |
| `TryAgain`, `Reload` | `main.tsx` the last-resort `ErrorBoundary`, which draws App anew while src-tauri's `State` runs on; in SaveFlow (`Reload`) also a page that loads again |
| `Relaunch` | A quit and a relaunch: `lib.rs` `run`'s setup, a new `State` whose `Removals::open` reads the marker (`removal.rs`, `read_marker`, `process`) |
| `ScreenReads` | `Automatic.tsx` `useCli("service:...")`, Refresh, `closeSave`, and the turn-off dialog's close, which reads again since a turn-off that failed may have stopped the job |
| `YouOpenSave`, `YouOpenTurnOff` | `Automatic.tsx` the switch's `onChange` and "Change folders" (`setDialog`) |
| `DialogReads` | `TurnOnDialog` `useCli("save-dialog")`; `service.rs` `read`, with `residue service status` run by `installs_as` and its `fingerprint:` line (`fingerprint_in`), and `plan`. `read` also runs `launchctl print` for the job launchd holds (`launchd_job`), which the window only shows when it differs from the agent file: it is not in the basis, so the model leaves it out |
| `YouTick` | `SaveForm` `toggle` and `add`, which hide a preview shown; the folder checkboxes and "Add a folder…", `disabled={locked}` while Preview or Save runs |
| `YouPreview`, `PreviewLands`, `PreviewFails` | `SaveForm` `preview`, `api.servicePreview(roots)`; the Preview button's `disabled` |
| `YouSave` | `SaveForm` `install`, which sends `status.basis`; the Save button's `disabled`, `plan.blocked` among it; the dialog's description |
| `YouTurnOff` | `TurnOffDialog` `off` |
| `YouCancel` | `TurnOnDialog` and `TurnOffDialog` `onOpenChange`, closed only while not `working` |
| `YouLeave` | `App.tsx` `go`, from Command-1 to 5 and the sidebar, which returns while `changingService()`; `api.ts` `change`, around `serviceInstall` and `serviceUninstall` |
| `InstallChecks` | `service.rs` `install`: `ctx.changes.start()` (`Changes`, held by `State` in `lib.rs`), `read`, `current.basis != *seen` returning `Save::Changed`, then `plan.blocked`; `Basis` and its fields, `fingerprint` among them; `lib.rs` `bundled_version_now`, which each service command reads again (`Shown`, `CachedVersion`) |
| `InstallCopies` | `service.rs` `install`: `Previous::keep`, then `copy_sidecar`, which writes the copy to a new file (`create_new`) and runs its `--version` against `installer.version` before it renames it into place (`NoCopyCheck`) |
| `InstallLands`, `InstallFails` | `service.rs` `install`: `--expect` with `seen.fingerprint` (none: an error, nothing run), `watch_args` with `plan.keep`, `copy_sidecar`, the install command, exit 4 (`EXPECT_CHANGED`) read again into `Save::Changed`, the `status` read after it; `Changing`'s drop on return. residue-cli's `service install --expect`: `service::check` under `service::lock`, right before the write; `Previous::restore` when residue wrote nothing (exit 4, or another failure with the agent file's bytes the same), `Previous::forget` when it wrote (`PutBack`, `NoPutBack`) |
| `UninstallStarts`, `UninstallLands`, `UninstallFails` | `service.rs` `uninstall`, `ctx.changes.start()` first |
| `ReplyArrives` | `SaveForm` `install`'s result: `onChanged`; for `changed`, the fresh read through `onAdopt` and `adopted`; `onFailed` |
| `Change`, `Terminal...`, `BrewUpgrade`, `AppReplaced` | No code: a commit or a branch outside the app; `residue service install` or `uninstall` in a terminal; `brew upgrade`; a new app copied over this one while it runs |

Sync commit: `18e1f267886bac5a860a86c90bea791353692332`, the head of #27 as stacked on #28 and #23, which was confirmed against the models on 2026-11-10. Move it forward only after you confirm, for each covered function a change touched, that the model still describes it. `git diff 18e1f26788 -- src/main.tsx src/App.tsx src/lib/api.ts src/views/RemoveDialog.tsx src/views/Worktrees.tsx src/views/Detail.tsx src/views/Automatic.tsx src-tauri/src/lib.rs src-tauri/src/service.rs src-tauri/src/cli.rs src-tauri/src/removal.rs` lists what to read. Every change to that code since the sync commit before it, `dfe663c64b` (the fix for #9 and #10), was confirmed with the models. The fix for #12: `Removals` in `removal.rs` and `lib.rs`, `removal_running` and `wait_for_removal`, `Run::output_with` in `cli.rs`, `watchRemoval` in `App.tsx`, the move buttons' `inFlight` in `Worktrees.tsx` and `Detail.tsx`, and the dialog's text for kind `invalid`. The fix for #13: `AsOf` and `remove_to_trash`'s `as_of` in `lib.rs`, `removeToTrash` in `api.ts`, and `confirm` in `RemoveDialog.tsx`, which sends the plan's `generated_at`; the paths a confirm sends are as before, and the model has no clock (Not covered). The fix for #11: `Basis.fingerprint` and `fingerprint_in`, `read`, which runs `residue service status` with `installs_as` before it reads the agent file, `run_preview`, which uses `installs_as` too, `install`'s `--expect` and `EXPECT_CHANGED` in `service.rs`, and `ServiceBasis` in `src/lib/types.ts`. The fix for Findings 3 and 4 (#28): `bundled_version_now` and the four service commands that call it in `lib.rs`, and `copy_sidecar`, `remove_leftovers`, `create_new`, `Previous` and `install` in `service.rs`, which `save/copy` models (`InstallCopies`, `PutBack`, `Now`). #23 changed how `Detail.tsx` and `cli.rs` quote the commands they show, which sends residue nothing new. #27 added `launchd_job` to `read`, which the screens only show, never `Basis` or `plan`, and the turn-off dialog's close reads the screen again, as `ScreenReads` already may at any time.

## Read a result

Read every result with `/statechart`. Give it a model, its chart config and its view: `model/RemoveFlow.tla` with `model/RemoveChart.cfg` and `model/remove-view.mjs`, or `model/SaveFlow.tla` with `model/SaveChart.cfg` and `model/save-view.mjs`. It tells a break as numbered steps in plain words, such as "you confirm", with the window's state after each, and draws a chart of a setup that passes. For a setup or a mutant, copy a chart config and set its constants and `Broken` as check.sh's `constants_of` and `model/mutants` do.

The two charts it drew are `model/remove.mmd` (one worktree: 10 states, 14 arrows) and `model/save.mmd` (one folder and a terminal that can turn the service on: 9 states, 24 arrows; `SaveChart.cfg` bounds it with `ChartBound` to a folder ticked and no preview, which keeps it under the 500 states `/statechart` draws, since a preview stays in "Shows the service as it is"). They hide steps that change nothing you see: nothing left to happen (`Finished`), Cancel while the save dialog reads (it does what Cancel on the form does), and steps within one state, such as a tick or the app's ask whether a removal runs. Draw them again when a model or a view changes. The fix for #12 left `remove.mmd` as it was. `RemoveChart.cfg` has no fault, so no removal runs when the window asks and none is refused. There the model reaches the same 125 states as before once `heard` and `app` are set aside (TLC with a `VIEW` of the other variables), and the steps the fix adds change only `heard`, so the chart has the same states and arrows. The fix for #11 left `save.mmd` as it was: `SaveChart.cfg` sets `DuringSave` to FALSE and allows no argument the app cannot read, TLC reaches the same 728 states (345 distinct) with it before and after, and `odd`, which the view now compares, is FALSE in every one of them. The fix for Findings 3 and 4 left it as it was too: `SaveChart.cfg` sets `Copy` to FALSE, and TLC reaches the same 728 states (345 distinct).

In check.sh's output:

- `+` against `model/expected` is a new break, or a known one with a new shortest path.
- `-` alone is a known break that no longer happens: a fix, or a model that lost the path.
- `mutant NAME: NOT caught` means the model no longer depends on that guard of the code.
- `no control` means the mutant's rule already breaks in its setup without the mutant, so the line shows nothing.

Each mutant in `model/mutants` removes one guard the code has, or two joined by `+`, to show that a rule depends on it. check.sh prints the shortest path to each break.

| Mutant | The guard it removes | Rule that breaks |
| --- | --- | --- |
| `PlanWithYes` | the plan runs without `--yes` (`remove`, `confirmed = None`) | `OnlyListedWorktreesMove` |
| `ConfirmSendsChoice` | the confirm sends the plan's `ready` paths, not the paths you chose | `ConfirmSendsTheList`, `OnlyListedWorktreesMove` |
| `ChosenUnfiltered` | `chosen` keeps only ticks the shown scan calls candidates | `OpensOnlyForCanGo` |
| `ChecksButtonForEveryVerdict` | the checks show the move button only for a candidate | `OpensOnlyForCanGo` |
| `NotModal` | the dialog is modal | `ListIsForYourChoice` |
| `RunErrorSaysNothingMoved` | a failed run keeps the plan and says to scan again | `NothingMovedIsTrue` |
| `ConfirmTwice+ConfirmWhileBusy` | the confirm is disabled while the run runs, with src-tauri's refusal gone too | `OneRemovalAtATime` |
| `CloseWhileBusy+ConfirmWhileBusy` | the dialog does not close while it plans or runs (`busy`), with src-tauri's refusal gone too | `OneRemovalAtATime` |
| `NoRunGuard` | src-tauri holds this app's confirmed run from before residue starts until `remove` returns (`Removals`), so it refuses a second one and tells the window that one runs; run in `remove/draw` and `remove/reload` | `OneRemovalAtATime` |
| `NoMarker` | the marker naming residue's process, which an app that opens again reads (`Removals::open`); run in `remove/reload` | `OneRemovalAtATime` |
| `ConfirmWhileBusy` | src-tauri refuses a confirm while a removal is in flight (`Removals::start`); run in `remove/draw` and `remove/reload`, where an ask that fails turns Move to the trash on | `OneRemovalAtATime` |
| `NoExpect` | `install` passes the dialog's fingerprint as `--expect`, so residue writes only while the service still has it; run in `save/during` | `SaveIsWhatTheDialogShowed`, `ReplacesOnlyWhatItShowed`, `NeverDropsAnArgument` |
| `NoBasisCheck+NoExpect` | `install` compares its read with the basis the dialog sent | `ReplacesOnlyWhatItShowed`, `SaveIsWhatTheDialogShowed` |
| `BasisWithoutMode+NoExpect` | the basis holds the service's `--dry-run` | `ReplacesOnlyWhatItShowed` |
| `BasisWithoutVersion+NoExpect` | the basis holds the version a save installs with, in `version` and in the release its fingerprint names (`fp`), which `install`'s comparison also reads | `SaveIsWhatTheDialogShowed` |
| `RetryOnChange` | a changed service saves nothing until you click again | `ReplacesOnlyWhatItShowed` |
| `SendsTheScreenBasis` | the dialog plans from its own read, not the screen's | `ReplacesOnlyWhatItShowed` |
| `NoBlockedCheck+NoBasisCheck+NoExpect` | `install` refuses a blocked plan | `NeverDropsAnArgument` |
| `SwitchInstalls` | the switch opens the dialog, never installs at once | `WarnedBeforeItDeletes` |
| `ChoicesWhileBusy` | the folder choices are disabled while Preview or Save runs (`locked`) | `PreviewIsOfTheTickedFolders`, `SaveIsOfTheTickedFolders` |
| `LeaveWhileSaving` | `go` keeps the screen while a save or turn-off runs | `TheDialogStaysWhileItSaves` |
| `NoChangeGuard` | `install` and `uninstall` refuse while another runs (`Changes`); run in `save/reload` | `OneChangeAtATime`; with `+NoExpect`, `ReplacesOnlyWhatItShowed` |
| `NoPutBack` | a save that writes nothing puts the app's copy back (`Previous::restore`); run in `save/copy` | `FailedSaveKeepsTheResidue` |
| `CachedVersion+NoCopyCheck` | each service command reads the bundled version again (`bundled_version_now`), with the copy check gone too | `SaveIsWhatTheDialogShowed` |
| `NoCopyCheck+NoExpect` | `copy_sidecar` copies only the residue the save showed, with `--expect` gone too | `SaveIsWhatTheDialogShowed` |

Some guards are backed by another, so removing one alone breaks no rule. The checkbox shown only for a candidate is backed by `chosen`, and the Save button disabled for a blocked plan is backed by `install`'s own blocked check: `ChosenUnfiltered` and `NoBlockedCheck+NoBasisCheck+NoExpect` remove the backing guard, with the first still in place. `--expect` backs `install`'s own read: residue compares the service with the dialog's read again right before it writes, and the fingerprint covers the agent file's bytes, an argument the app cannot read among them. So the basis check, each of its fields, the blocked check, and `Changes` for `ReplacesOnlyWhatItShowed` keep their rules only once `NoExpect` is gone too, as the `+NoExpect` lines show, and the basis check backs the blocked check the same way. `NoExpect` alone breaks a rule only in `save/during`, where nothing else stands between `install`'s read and residue's write. src-tauri's refusal backs three guards of the window: the confirm disabled while the run runs, the dialog that stays while it runs, and Move to the trash off while a removal runs (`MoveOn`). The first two keep the rule once the refusal is gone, as `ConfirmTwice+ConfirmWhileBusy` and `CloseWhileBusy+ConfirmWhileBusy` show. `MoveOn` has no mutant: an ask that fails turns Move to the trash on, so `MoveOn` cannot keep the rule alone. The refusal keeps it, as `ConfirmWhileBusy` shows. In `save/copy`, the copy check backs both the fresh version read and `--expect`: a read that names the version the app started with is caught when the copy prints another, and so is a new app put in place between `install`'s read and the copy, which `--expect` also refuses, since the copy then runs the install and names its own release. So `CachedVersion` and `NoCopyCheck` alone break no rule.

## A bug, or a modeling error

Replay the path step by step against the code. For each action, open its code in the table and check that the code can take that step in that state. A step the code cannot take is a modeling error: fix the model, and say in the change what was wrong. When every step can happen, it is a bug in the code: report it with the steps in the code's words, and the harm in one sentence.

To witness a removal path, follow AGENTS.md, "Verify a change": scratch repositories in a temporary folder, never real ones. A save path cannot be witnessed on a Mac that runs residue's service, because `residue service install` and `uninstall` act on the real login session; replay it against the code.

## Change the model with the code

1. Update the actions for each covered function you changed, from the code, before you rerun. Keep one indivisible step of the code one action.
2. A new guard gets a `Mut("NAME")` switch in the model and a line in `model/mutants`. check.sh then shows that the rule holds with the guard and breaks without it.
3. A fix for a finding removes its lines from `model/expected`, with the reason, in the same change. The rule must then hold in that setup.
4. Never add a line to `model/expected`, or weaken a rule, to make the check pass. A new break is a finding until a replay against the code shows it is a modeling error.
5. Run the check, follow it with `/statechart` for each new or changed line, and move the sync commit.

## Findings

None is open: every rule holds in every setup, and `model/expected` lists no break. A finding keeps its lines in `model/expected` until a fix removes them. A replay is a walk through the code; none was run in the app.

**1. A second confirmed removal could start while the first still ran. Fixed (#12).** It needed a fault. You confirmed a removal, and while `residue remove --trash --yes` ran, you quit and opened the app again (the app does not stop residue when it quits), the last-resort `ErrorBoundary` in `main.tsx` drew App anew, or the removal dialog failed to draw and its Close scanned again. The new scan still showed the worktree as can go, and nothing in the app or in `src-tauri` knew a run was in flight, so you could confirm a second run of the same path. Both runs sent only the paths their own lists showed, and residue checked each path again, but residue-cli's model does not cover two remove runs at once.

The fix keeps the guard in the app, across launches:

- `remove` holds `Removals` (`removal.rs`) from before residue starts until it returns, and refuses a second confirmed removal meanwhile with kind `invalid`, running nothing. The dialog says "This removal did not start, so nothing moved."
- While residue runs, a marker in the app's data folder names its process by PID, start time to the microsecond, and name. An app that opens again counts a removal whose process still runs as in flight, and removes a marker whose process has ended. A PID goes to another process only after its process exited, and that one starts later, so a reused PID never keeps a removal in flight.
- The window asks src-tauri when it starts and when the removal dialog closes, and keeps Move to the trash off, with "A removal is still running.", until that removal ends. Then it scans again.

The two lines left `model/expected` with the fix. `NoRunGuard`, `NoMarker` and `ConfirmWhileBusy` show what each part keeps.

**2. A save could replace a service that changed after its last check. Fixed (#11).** `install` compared the service with the dialog's read, then ran `residue service install`, and nothing tied that install to what it compared. A `brew upgrade`, or a terminal's `residue service install` that turned the service on or added a folder or an option, in between went unseen. The models had no such change ("A terminal or brew changes the service only while no `install` or `uninstall` call of the app runs"), so they could not find it. With the change possible there (`save/during`, with `NoExpect`), three rules break, each by TLC's shortest path, and `WarnedBeforeItDeletes` holds:

| Change between `InstallChecks` and `InstallLands` | Rule that breaks |
| --- | --- |
| `BrewUpgrade` | `SaveIsWhatTheDialogShowed` |
| `TerminalTurnsOn` | `ReplacesOnlyWhatItShowed` |
| `TerminalAddsArgument` | `NeverDropsAnArgument` |

The fix makes the comparison and the write one step, in residue: `residue service status` prints a fingerprint of the service (residue-cli 0.0.12), the dialog's read keeps it in `Basis`, and `install` passes it as `residue service install --expect`. residue compares it with the service under a lock that every `service install` and `uninstall` takes, right before it writes, and on a mismatch writes nothing and exits 4, which `install` returns as `Save::Changed` with a fresh read. In `save/during` every rule then holds. The `+NoExpect` mutant lines show which of the app's own guards `--expect` backs.

**3. A save that installed nothing could still change which residue the service runs. Fixed.** With the app's own copy, `install` wrote the bundled residue over that copy, then ran `residue service install`, and returned at once when residue refused before it wrote the agent file. The service kept its folders and mode, but the copy at the path it runs was now the bundled release; at its next start, or its next pass, it ran that one. The model had one `ver` for the residue a save installs with and the one the service runs, and its `InstallFails` changed neither, so it could not show this. `save/copy` adds the copy as `run`, the copy step `InstallCopies`, and the rule `FailedSaveKeepsTheResidue`; without the put-back (`NoPutBack`) it breaks by `YouOpenSave AppReplaced DialogReads YouSave InstallChecks InstallCopies InstallFails`. The fix keeps the copy under a second name, a hard link or, on a disk without them such as exFAT, a copy, until residue has written the agent file, and puts it back when residue wrote nothing: it exited 4, which `--expect` promises means nothing was written, or it failed otherwise with the agent file's bytes the same. A save that leaves the bytes as they were but still ran residue's launchd steps says only that residue could not install the service, never that nothing changed.

**4. After a new app was put in place of this one while it ran, a save could name one residue and install another. Fixed.** The bundled version came from a cache filled by the first read after launch, and every service command planned with it, so the dialog named the release the app started with while the copy a save wrote was the new one; the fingerprint, read by the new binary, matched it. `save/copy` shows it with `CachedVersion+NoCopyCheck`: `YouOpenSave AppReplaced DialogReads YouSave InstallChecks InstallCopies InstallLands` breaks `SaveIsWhatTheDialogShowed`. Each service command now reads `residue --version` again (`bundled_version_now`; the window's version line keeps the cache), and `copy_sidecar` runs the new copy's `--version` and refuses one that is not the release the save showed, before it renames it into place. The copy is opened as a new file, with `O_EXCL` and `O_NOFOLLOW`.

## Restore

Restore is not modeled, because it cannot move or overwrite more than its row. The app runs `residue restore --json ENTRY` (`lib.rs` `restore`) for the one row you clicked, with no `--to`, and disables every Restore button while one runs (`Restore.tsx`, `working`). residue refuses to restore onto a path that exists ("exists; move it away first, or restore somewhere else with --to PATH", in residue-cli's `trash.rs` and `removed.rs`), except a record whose worktree is back, where it adds only what the worktree lacks of the record. What a restore does to a worktree is residue's, and residue-cli's `verify-removal-races` models both kinds.

## Not covered

- The trash entries a remove run purges, and the removal records it forgets: the model's runs only move the paths they are sent.
- The clock, and so the time between the plan and the confirmed run: the confirmed run passes the shown plan's `generated_at` as `--as-of`, so residue judges every age at the plan's time (#13).
- What the preview, `residue watch --once --dry-run`, finds: it removes nothing, so the model keeps only the folders it ran on.
- The sidebar and Command-1 to 5 in the removal model, which stays on its own screen. The save model leaves its screen through `go`.
- A terminal or an editor that writes the agent file while a save that copied fails with an exit status other than 4: the app takes the change for residue's and keeps the new copy, since only residue's exit 4 says it wrote nothing.
- A change to the service by something that takes no lock, between residue's comparison and its write: a residue older than 0.0.12, an editor of the agent file, or `launchctl` run by hand.
- A quit and a relaunch while a save or turn-off runs. `Changes` lives in the app's process, and nothing stops a `residue service install` the old app started (`cli.rs`), so a relaunched app can start a second one beside it. residue's lock runs the two one after the other, and the second, with `--expect`, writes only over the service its own dialog showed, as `NoChangeGuard+NoExpect` shows within one app; but the app has no marker for a save, as it has for a removal (Finding 1), so the relaunched window does not know one runs.
- The save dialog's own read failing, which shows residue's error with Try again and Cancel, and no Save.
- residue's own checks and moves, and `residue watch`.
- The app quitting between residue's start and the marker's rename, a few system calls apart, or a marker that cannot be written once residue runs, which src-tauri says on stderr: an app that opens again then does not see that removal. A data folder the app cannot write refuses the removal before residue starts, since src-tauri creates the marker's `.part` file first.
- A residue that an earlier launch started and that hangs: the window waits for it with no deadline, since the deadline belonged to the app that quit. It ends when that residue exits or is stopped.
- Two windows, two copies of the app at once (each has its own `Removals`, and reads the marker only when it opens), and two faults in one behavior.

## Credit

The method follows Lauren Tan's pstack skills, `create-formal-verification-skill` and `statechart`, in [cursor/plugins](https://github.com/cursor/plugins) (MIT).
