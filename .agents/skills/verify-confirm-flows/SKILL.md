---
name: verify-confirm-flows
description: Model-check every order of your clicks, residue's replies and changes outside the app in the two flows where the app moves or deletes something, moving worktrees to the trash and turning automatic cleanup on, saving its folders or turning it off, to show whether any order moves a worktree the confirm did not list or installs a service the dialog did not show. Use before you merge a change to src/views/RemoveDialog.tsx, the removal dialog's wiring, `watchRemoval` or `go` in src/App.tsx, `changingService` in src/lib/api.ts, `chosen`, the checkboxes or the move buttons in src/views/Worktrees.tsx and src/views/Detail.tsx, `remove`, `plan_removal`, `remove_to_trash`, `removal_running` or `wait_for_removal` in src-tauri/src/lib.rs, src-tauri/src/removal.rs, `Basis`, `read`, `plan`, `install`, `uninstall` or `Changes` in src-tauri/src/service.rs, or the save and turn-off dialogs in src/views/Automatic.tsx, and when someone asks whether a click can move or delete more than the app showed.
---

# Verify the confirm flows

The app moves worktrees to the trash in two runs of residue: a plan that moves nothing, then, after your click, the same command with `--yes` and the planned paths. It saves automatic cleanup from a dialog that reads the service, shows it, and sends that read back, so that `install` installs nothing when the service has changed. Between those steps you can click, residue can reply late or fail, the window can start afresh or the app quit and open again, and a terminal or `brew upgrade` can change the service. This check tries every order of those steps, for two worktrees and for two folders, and says which of fourteen rules any order breaks.

It is two TLA+ models of the app, `model/RemoveFlow.tla` and `model/SaveFlow.tla`, checked with TLC. TLC does not read the TypeScript or the Rust: a model holds only while it describes the code, which is why the coverage table and the sync commit below exist.

## Run it

```sh
bash .agents/skills/verify-confirm-flows/check.sh
```

It takes about 90 seconds and exits 0 when the result matches `model/expected`: every rule holds except the known breaks it lists, each with its shortest path, and every mutant is caught. It exits 1 when the result differs, and 2 when the tools cannot be set up or TLC fails.

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
- The app and residue: the dialog's own read, the preview and its reply, `Changes`, which refuses an `install` or `uninstall` while another runs, `install`'s read and its comparison with the basis the dialog sent, the blocked check, `residue service install` and its reply, and `residue service uninstall` and its reply. Each reply can fail, also after it wrote the service.
- Outside the app, twice at most: a terminal turns the service on or off, changes its folders, its mode or its options, or adds an argument the app cannot read, or `brew upgrade` puts a newer residue where a save installs from.
- Faults, at most one, in `save/reload`: the window starts afresh while a save or turn-off runs on in src-tauri (the page loads again, or the last-resort "Try again"). A quit and a relaunch is not modeled here: it starts a new `Changes` (Not covered).

| Setup | Model | Faults | States | Distinct states |
| --- | --- | --- | --- | --- |
| `remove/no-faults` | RemoveFlow | none | 171,405 | 46,464 |
| `remove/reload` | RemoveFlow | the window starts afresh, or the app quits and opens again | 2,758,623 | 521,382 |
| `remove/draw` | RemoveFlow | the dialog fails to draw | 1,335,889 | 274,416 |
| `save/outside` | SaveFlow | none; every outside change | 8,907,360 | 2,878,776 |
| `save/reload` | SaveFlow | the window starts afresh while a save or turn-off runs; one folder, no outside change | 77,392 | 24,243 |

Each setup opens a dialog at most twice. Every rule holds in every setup, so each count is of one run with every rule. When a rule breaks, TLC stops at its first break and reports the shortest path, and check.sh checks the other rules again without it: a rule that breaks in a setup is not checked further there, and a new way to break it shows only if it is shorter.

Assumptions:

- Each click, read and reply is one indivisible step: React runs a handler and its state updates whole.
- A reply lands in the dialog that asked for it, and only while that dialog is open: RemoveDialog's `live` flag, and the latest request in `useCli`. A removal's report still reaches App's `onRemoved` after its dialog closed.
- Each dialog is modal, and App ignores Command keys while the removal dialog is open (`if (removing) return`). The `NotModal` mutant shows what that guard keeps. Command-1 to 5 still reach `go` while the save dialog is open; `go` refuses another screen while a save or turn-off runs (`changingService`), and `LeaveWhileSaving` shows what that keeps.
- "Add a folder…" opens the folder picker as a sheet on the window (tauri-plugin-dialog sets the window as its parent; rfd then calls `beginSheetModalForWindow`), which takes no other click while it is open, so adding a folder is one step.
- residue checks every path again on each run and moves only what can still go. The model trusts it; residue-cli's `verify-removal-races` checks residue.
- A confirmed run replies after residue exited: `cli.rs` waits for it, and at the deadline stops it and waits. When that wait fails, residue may still run, and src-tauri keeps the run in flight until its process ends (`removal.rs`, `Left`).
- src-tauri's answer to the window's ask is read when it lands. While the window asks, Move to the trash is off, so no run starts in between; a run that ends in between can only shorten the wait.
- A terminal or brew changes the service only while no `install` or `uninstall` call of the app runs.
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
| `ScreenReads` | `Automatic.tsx` `useCli("service:...")`, Refresh, `closeSave` |
| `YouOpenSave`, `YouOpenTurnOff` | `Automatic.tsx` the switch's `onChange` and "Change folders" (`setDialog`) |
| `DialogReads` | `TurnOnDialog` `useCli("save-dialog")`; `service.rs` `read`, `plan` |
| `YouTick` | `SaveForm` `toggle` and `add`, which hide a preview shown; the folder checkboxes and "Add a folder…", `disabled={locked}` while Preview or Save runs |
| `YouPreview`, `PreviewLands`, `PreviewFails` | `SaveForm` `preview`, `api.servicePreview(roots)`; the Preview button's `disabled` |
| `YouSave` | `SaveForm` `install`, which sends `status.basis`; the Save button's `disabled`, `plan.blocked` among it; the dialog's description |
| `YouTurnOff` | `TurnOffDialog` `off` |
| `YouCancel` | `TurnOnDialog` and `TurnOffDialog` `onOpenChange`, closed only while not `working` |
| `YouLeave` | `App.tsx` `go`, from Command-1 to 5 and the sidebar, which returns while `changingService()`; `api.ts` `change`, around `serviceInstall` and `serviceUninstall` |
| `InstallChecks` | `service.rs` `install`: `ctx.changes.start()` (`Changes`, held by `State` in `lib.rs`), `read`, `current.basis != *seen` returning `Save::Changed`, then `plan.blocked`; `Basis` and its fields |
| `InstallLands`, `InstallFails` | `service.rs` `install`: `watch_args` with `plan.keep`, `copy_sidecar`, the install command, the `status` read after it; `Changing`'s drop on return |
| `UninstallStarts`, `UninstallLands`, `UninstallFails` | `service.rs` `uninstall`, `ctx.changes.start()` first |
| `ReplyArrives` | `SaveForm` `install`'s result: `onChanged`; for `changed`, the fresh read through `onAdopt` and `adopted`; `onFailed` |
| `Change`, `Terminal...`, `BrewUpgrade` | No code: a commit or a branch outside the app; `residue service install` or `uninstall` in a terminal; `brew upgrade` |

Sync commit: `dfe663c64b293afa71a0b6d70f7474e98a39e6a9`, the fix for #9 and #10, which was confirmed against the models. The models were last confirmed to describe the code above at that commit, on 2026-11-09. Move it forward only after you confirm, for each covered function a change touched, that the model still describes it. `git diff dfe663c64b -- src/main.tsx src/App.tsx src/lib/api.ts src/views/RemoveDialog.tsx src/views/Worktrees.tsx src/views/Detail.tsx src/views/Automatic.tsx src-tauri/src/lib.rs src-tauri/src/service.rs src-tauri/src/cli.rs src-tauri/src/removal.rs` lists what to read. The change that fixed #12 is in that diff and was confirmed with it: `Removals` in `removal.rs` and `lib.rs`, `removal_running` and `wait_for_removal`, `Run::output_with` in `cli.rs`, `watchRemoval` in `App.tsx`, the move buttons' `inFlight` in `Worktrees.tsx` and `Detail.tsx`, and the dialog's text for kind `invalid`. The change that fixed #13 is in that diff too and was confirmed with it: `AsOf` and `remove_to_trash`'s `as_of` in `lib.rs`, `removeToTrash` in `api.ts`, and `confirm` in `RemoveDialog.tsx`, which sends the plan's `generated_at`. The paths a confirm sends are as before, and the model has no clock (Not covered).

## Read a result

Read every result with `/statechart`. Give it a model, its chart config and its view: `model/RemoveFlow.tla` with `model/RemoveChart.cfg` and `model/remove-view.mjs`, or `model/SaveFlow.tla` with `model/SaveChart.cfg` and `model/save-view.mjs`. It tells a break as numbered steps in plain words, such as "you confirm", with the window's state after each, and draws a chart of a setup that passes. For a setup or a mutant, copy a chart config and set its constants and `Broken` as check.sh's `constants_of` and `model/mutants` do.

The two charts it drew are `model/remove.mmd` (one worktree: 10 states, 14 arrows) and `model/save.mmd` (one folder and a terminal that can turn the service on: 9 states, 24 arrows; `SaveChart.cfg` bounds it with `ChartBound` to a folder ticked and no preview, which keeps it under the 500 states `/statechart` draws, since a preview stays in "Shows the service as it is"). They hide steps that change nothing you see: nothing left to happen (`Finished`), Cancel while the save dialog reads (it does what Cancel on the form does), and steps within one state, such as a tick or the app's ask whether a removal runs. Draw them again when a model or a view changes. The fix for #12 left `remove.mmd` as it was. `RemoveChart.cfg` has no fault, so no removal runs when the window asks and none is refused. There the model reaches the same 125 states as before once `heard` and `app` are set aside (TLC with a `VIEW` of the other variables), and the steps the fix adds change only `heard`, so the chart has the same states and arrows.

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
| `NoBasisCheck` | `install` compares its read with the basis the dialog sent | `ReplacesOnlyWhatItShowed`, `SaveIsWhatTheDialogShowed` |
| `BasisWithoutMode` | the basis holds the service's `--dry-run` | `ReplacesOnlyWhatItShowed` |
| `BasisWithoutVersion` | the basis holds the version a save installs with | `SaveIsWhatTheDialogShowed` |
| `RetryOnChange` | a changed service saves nothing until you click again | `ReplacesOnlyWhatItShowed` |
| `SendsTheScreenBasis` | the dialog plans from its own read, not the screen's | `ReplacesOnlyWhatItShowed` |
| `NoBlockedCheck` | `install` refuses a blocked plan | `NeverDropsAnArgument` |
| `SwitchInstalls` | the switch opens the dialog, never installs at once | `WarnedBeforeItDeletes` |
| `ChoicesWhileBusy` | the folder choices are disabled while Preview or Save runs (`locked`) | `PreviewIsOfTheTickedFolders`, `SaveIsOfTheTickedFolders` |
| `LeaveWhileSaving` | `go` keeps the screen while a save or turn-off runs | `TheDialogStaysWhileItSaves` |
| `NoChangeGuard` | `install` and `uninstall` refuse while another runs (`Changes`); run in `save/reload` | `OneChangeAtATime`, `ReplacesOnlyWhatItShowed` |

Some guards are backed by another, so removing one alone breaks no rule. The checkbox shown only for a candidate is backed by `chosen`, and the Save button disabled for a blocked plan is backed by `install`'s own check: `ChosenUnfiltered` and `NoBlockedCheck` remove the backing guard, with the first still in place. src-tauri's refusal backs three guards of the window: the confirm disabled while the run runs, the dialog that stays while it runs, and Move to the trash off while a removal runs (`MoveOn`). The first two keep the rule once the refusal is gone, as `ConfirmTwice+ConfirmWhileBusy` and `CloseWhileBusy+ConfirmWhileBusy` show. `MoveOn` has no mutant: an ask that fails turns Move to the trash on, so `MoveOn` cannot keep the rule alone. The refusal keeps it, as `ConfirmWhileBusy` shows.

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

## Restore

Restore is not modeled, because it cannot move or overwrite more than its row. The app runs `residue restore --json ENTRY` (`lib.rs` `restore`) for the one row you clicked, with no `--to`, and disables every Restore button while one runs (`Restore.tsx`, `working`). residue refuses to restore onto a path that exists ("exists; move it away first, or restore somewhere else with --to PATH", in residue-cli's `trash.rs` and `removed.rs`), except a record whose worktree is back, where it adds only what the worktree lacks of the record. What a restore does to a worktree is residue's, and residue-cli's `verify-removal-races` models both kinds.

## Not covered

- The trash entries a remove run purges, and the removal records it forgets: the model's runs only move the paths they are sent.
- The clock, and so the time between the plan and the confirmed run: the confirmed run passes the shown plan's `generated_at` as `--as-of`, so residue judges every age at the plan's time (#13).
- What the preview, `residue watch --once --dry-run`, finds: it removes nothing, so the model keeps only the folders it ran on.
- The sidebar and Command-1 to 5 in the removal model, which stays on its own screen. The save model leaves its screen through `go`.
- A terminal or brew changing the service while a save or a turn-off of the app runs.
- A quit and a relaunch while a save or turn-off runs. `Changes` lives in the app's process, and nothing stops a `residue service install` the old app started (`cli.rs`), so a relaunched app can start a second one beside it. A removal has a marker for this (Finding 1); a save has none.
- The save dialog's own read failing, which shows residue's error with Try again and Cancel, and no Save.
- residue's own checks and moves, and `residue watch`.
- The app quitting between residue's start and the marker's rename, a few system calls apart, or a marker that cannot be written once residue runs, which src-tauri says on stderr: an app that opens again then does not see that removal. A data folder the app cannot write refuses the removal before residue starts, since src-tauri creates the marker's `.part` file first.
- A residue that an earlier launch started and that hangs: the window waits for it with no deadline, since the deadline belonged to the app that quit. It ends when that residue exits or is stopped.
- Two windows, two copies of the app at once (each has its own `Removals`, and reads the marker only when it opens), and two faults in one behavior.

## Credit

The method follows Lauren Tan's pstack skills, `create-formal-verification-skill` and `statechart`, in [cursor/plugins](https://github.com/cursor/plugins) (MIT).
