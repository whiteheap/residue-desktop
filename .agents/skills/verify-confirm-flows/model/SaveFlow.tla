------------------------------ MODULE SaveFlow ------------------------------
(***************************************************************************)
(* How the app turns automatic cleanup on, saves other folders, and turns  *)
(* it off, as the window, residue and the world outside take turns: the    *)
(* Automatic cleanup screen and its two dialogs (src/views/Automatic.tsx), *)
(* the save form's folders and its Preview, leaving the screen (`go` in    *)
(* src/App.tsx, with `changingService` in src/lib/api.ts), and `install`   *)
(* and `uninstall` in src-tauri/src/service.rs, which run                  *)
(* `residue service install --expect FINGERPRINT [OPTIONS] [--dry-run]     *)
(* FOLDERS...` and `residue service uninstall`, with Copy after writing    *)
(* the app's own copy of the bundled residue. A terminal, `brew upgrade`   *)
(* or a new app put in place of this one can change the service, or the   *)
(* residue a save installs, while a dialog is open, and with DuringSave    *)
(* while a save runs. The coverage table in ../SKILL.md maps each action   *)
(* to the code.                                                            *)
(***************************************************************************)
EXTENDS FiniteSets, Naturals

CONSTANTS
  Folders,     \* the folders a save can watch, such as {"A", "B"}
  AppRoots,    \* the folders the app scans, where a service that is off starts
  Broken,      \* the guards removed, from model/mutants; {} is the code
  MaxOpens,    \* how many times a dialog opens
  MaxOutside,  \* how many changes a terminal or brew makes
  World,       \* which of them can happen: "on", "off", "folders", "mode",
               \* "options", "argument", "brew", "app"
  DuringSave,  \* TRUE: they can happen while a call of the app runs too (#11)
  Faults,      \* what may go wrong once in a behavior: "reload"
  Copy         \* TRUE: a save installs with the app's own copy of the bundled
               \* residue (`Installer.managed`), which it writes first

Mut(name) == name \in Broken

VARIABLES
  svc,   \* the service launchd runs: on, folders, --dry-run, options, and an
         \* argument the app cannot read (`Program.unknown`)
  ver,   \* the residue a save installs with (`Plan.binary`, `Plan.version`);
         \* with Copy, the bundled one, which a new app put in place of this
         \* one changes while it runs
  run,   \* with Copy, the residue in the app's own copy, which the service
         \* runs
  scr,   \* the read the Automatic cleanup screen shows
  dlg,   \* the open dialog: "on" (turn on or save folders), "off", or none;
         \* the folders the save form ticks, and its preview
  calls, \* the app's `service install` and `uninstall` calls, by slot
  last,  \* what the app's last install replaced and wrote
  n      \* counters: dialog opens, outside changes, faults

vars == <<svc, ver, run, scr, dlg, calls, last, n>>

\* Each opening of a dialog has at most one call in flight, so two slots
\* hold every call when a dialog opens at most twice.
Slots == {1, 2}
ASSUME MaxOpens <= Cardinality(Slots)

Opts == {"defaults", "set"}  \* residue's defaults, or options such as --grace 14d
Off == [on |-> FALSE, roots |-> {}, dry |-> FALSE, opts |-> "defaults", odd |-> FALSE]

\* The version a read names for the residue a save installs with. With Copy
\* it is the bundled residue's, read again for each service command
\* (`bundled_version_now` in lib.rs); with "CachedVersion" it is the one
\* the app read first, at its start, as before.
Shown(v) == IF Copy /\ Mut("CachedVersion") THEN "old" ELSE v

\* A read's `Basis`: the service a save replaces, the options it keeps, and
\* the residue it installs with. Only a service that is on has options to keep.
\* Its fingerprint (`Basis.fingerprint`, from `residue service status`) names
\* the agent file's bytes, so an argument the app cannot read is in it too,
\* and the release of the residue that read it (`fp`), which is the one a
\* save installs with, whatever version the read names (`ver`).
Basis(s, v) == [on |-> s.on, roots |-> s.roots, dry |-> s.dry, opts |-> s.opts, odd |-> s.odd,
                ver |-> Shown(v), fp |-> v]
Read == Basis(svc, ver)
\* What `residue service install --expect` compares: the fingerprint.
Fp(b) == [b EXCEPT !.ver = "none"]
\* The fingerprint as residue reads it right before it writes: the service
\* as it is, and the release of the residue that runs the install, with
\* Copy the one in the app's own copy.
Now == [Fp(Read) EXCEPT !.fp = IF Copy THEN run ELSE ver]
Keep(b) == IF b.on THEN b.opts ELSE "defaults"
\* `Plan.blocked`: a save would drop an argument the app cannot read.
Blocked == svc.on /\ svc.odd
\* `startFrom`: the running service's folders, else the app's.
StartRoots(b) == IF b.on /\ b.roots # {} THEN b.roots ELSE AppRoots

NoRead == [on |-> FALSE, roots |-> {}, dry |-> FALSE, opts |-> "none", odd |-> FALSE, ver |-> "none",
           fp |-> "none"]
\* The save form's preview: none shown, running for the folders ticked at
\* the click, or shown for the folders it ran on.
NoPreview == [phase |-> "none", roots |-> {}]
NoDialog == [kind |-> "none", phase |-> "none", shown |-> NoRead, blocked |-> FALSE,
             chosen |-> {}, pre |-> NoPreview]
\* `own`: the dialog that made the call is open and gets its reply. `page`:
\* the call counts in this window's `changingService`, until its reply.
\* `copied`: it wrote the app's own copy, which held `prev` before and
\* `after` once the call failed or found the service changed.
NoCall == [kind |-> "none", phase |-> "none", seen |-> NoRead, shown |-> NoRead,
           roots |-> {}, dry |-> FALSE, warned |-> FALSE, fresh |-> NoRead,
           freshBlocked |-> FALSE, keep |-> "none", ver |-> "none", wrote |-> FALSE,
           own |-> FALSE, page |-> FALSE, copied |-> FALSE, prev |-> "none", after |-> "none"]
NoLast == [set |-> FALSE, before |-> Off, after |-> Off, ver |-> "none", shown |-> NoRead,
           roots |-> {}, dry |-> FALSE]

Bases == [on : BOOLEAN, roots : SUBSET Folders, dry : BOOLEAN,
          opts : Opts \cup {"none"}, odd : BOOLEAN, ver : {"old", "new", "none"},
          fp : {"old", "new", "none"}]

\* A call inside `install` past its read, or inside `uninstall`: it holds
\* `Changes` in service.rs.
Runs(i) == calls[i].phase \in {"checked", "copied", "running"}
\* `Changes::start` refuses while another call holds it.
Refused(i) == ~Mut("NoChangeGuard") /\ \E j \in Slots \ {i} : Runs(j)
\* A save or turn-off of this window that has not settled (`changingService`).
Held == \E i \in Slots : calls[i].page
NoneOwn == \A i \in Slots : ~calls[i].own
Idle == \A i \in Slots : calls[i].phase = "none"
Free == CHOOSE i \in Slots : calls[i].phase = "none"

TypeOK ==
  /\ svc \in [on : BOOLEAN, roots : SUBSET Folders, dry : BOOLEAN, opts : Opts, odd : BOOLEAN]
  /\ ver \in {"old", "new"}
  /\ run \in {"old", "new"}
  /\ scr \in Bases
  /\ dlg.kind \in {"none", "on", "off"}
  /\ dlg.phase \in {"none", "reading", "form", "saving"}
  /\ dlg.shown \in Bases /\ dlg.blocked \in BOOLEAN
  /\ dlg.chosen \subseteq Folders
  /\ dlg.pre \in [phase : {"none", "running", "shown"}, roots : SUBSET Folders]
  /\ \A i \in Slots :
       /\ calls[i].kind \in {"none", "install", "uninstall"}
       /\ calls[i].phase \in {"none", "sent", "checked", "copied", "running", "landed", "changed",
                              "failed"}
       /\ calls[i].seen \in Bases /\ calls[i].shown \in Bases /\ calls[i].fresh \in Bases
       /\ calls[i].roots \subseteq Folders /\ calls[i].dry \in BOOLEAN
       /\ calls[i].warned \in BOOLEAN /\ calls[i].wrote \in BOOLEAN
       /\ calls[i].own \in BOOLEAN /\ calls[i].page \in BOOLEAN
       /\ calls[i].copied \in BOOLEAN
       /\ calls[i].prev \in {"old", "new", "none"} /\ calls[i].after \in {"old", "new", "none"}

Init ==
  /\ svc \in {Off, [on |-> TRUE, roots |-> AppRoots, dry |-> FALSE, opts |-> "set", odd |-> FALSE]}
  /\ ver = "old"
  /\ run = "old"
  /\ scr = Basis(svc, ver)
  /\ dlg = NoDialog
  /\ calls = [i \in Slots |-> NoCall]
  /\ last = NoLast
  /\ n = [opens |-> 0, outside |-> 0, faults |-> 0]

-----------------------------------------------------------------------------
(* The screen and the dialogs.                                              *)

\* Refresh, Command-R (also while a dialog is open), or a read after a
\* dialog closes.
ScreenReads ==
  /\ scr # Read
  /\ scr' = Read
  /\ UNCHANGED <<svc, ver, run, dlg, calls, last, n>>

\* The switch, or "Change folders": the save dialog opens and reads the
\* service itself. With "SwitchInstalls" the switch turns the service on for
\* the app's folders at once, with no dialog. A call whose dialog is gone
\* can still run (`own` false).
YouOpenSave ==
  /\ dlg.kind = "none" /\ NoneOwn
  /\ n.opens < MaxOpens
  /\ n' = [n EXCEPT !.opens = @ + 1]
  /\ IF Mut("SwitchInstalls") /\ ~scr.on
       THEN /\ calls' = [calls EXCEPT ![Free] =
                 [NoCall EXCEPT !.kind = "install", !.phase = "sent", !.seen = scr,
                                !.shown = scr, !.roots = AppRoots, !.warned = FALSE,
                                !.own = TRUE, !.page = TRUE]]
            /\ UNCHANGED dlg
       ELSE /\ dlg' = [NoDialog EXCEPT !.kind = "on", !.phase = "reading"]
            /\ UNCHANGED calls
  /\ UNCHANGED <<svc, ver, run, scr, last>>

\* The dialog's own read lands (`useCli("save-dialog")`). The form says the
\* service deletes instead of using the trash, whatever it shows, and ticks
\* the folders it starts from.
DialogReads ==
  /\ dlg.kind = "on" /\ dlg.phase = "reading"
  /\ dlg' = [dlg EXCEPT !.phase = "form", !.shown = Read, !.blocked = Blocked,
                        !.chosen = StartRoots(Read)]
  /\ UNCHANGED <<svc, ver, run, scr, calls, last, n>>

\* A folder's checkbox, or "Add a folder…", whose folder picker is a sheet
\* on the window that takes no other click while it is open. A change hides
\* the preview shown, but not one that runs: that one appears when it lands.
\* Disabled while Preview or Save runs; with "ChoicesWhileBusy" they are not.
YouTick ==
  /\ dlg.kind = "on"
  /\ \/ dlg.phase = "form" /\ dlg.pre.phase # "running"
     \/ Mut("ChoicesWhileBusy") /\ dlg.phase \in {"form", "saving"}
  /\ \E f \in Folders :
       dlg' = [dlg EXCEPT !.chosen = IF f \in @ THEN @ \ {f} ELSE @ \cup {f},
                          !.pre = IF @.phase = "shown" THEN NoPreview ELSE @]
  /\ UNCHANGED <<svc, ver, run, scr, calls, last, n>>

\* Preview: one pass of the service with --dry-run for the folders ticked at
\* the click. Disabled while Preview or Save runs, and with no folder.
YouPreview ==
  /\ dlg.kind = "on" /\ dlg.phase = "form" /\ dlg.pre.phase # "running"
  /\ dlg.chosen # {}
  /\ dlg' = [dlg EXCEPT !.pre = [phase |-> "running", roots |-> dlg.chosen]]
  /\ UNCHANGED <<svc, ver, run, scr, calls, last, n>>

PreviewLands ==
  /\ dlg.kind = "on" /\ dlg.pre.phase = "running"
  /\ dlg' = [dlg EXCEPT !.pre.phase = "shown"]
  /\ UNCHANGED <<svc, ver, run, scr, calls, last, n>>

\* The form shows residue's error.
PreviewFails ==
  /\ dlg.kind = "on" /\ dlg.pre.phase = "running"
  /\ dlg' = [dlg EXCEPT !.pre = NoPreview]
  /\ UNCHANGED <<svc, ver, run, scr, calls, last, n>>

\* "Save folders" or "Turn on automatic cleanup", with the folders ticked and
\* either mode. Disabled when the plan is blocked, with no folder, and while
\* Preview runs. It sends the basis of the read the dialog shows.
YouSave ==
  /\ dlg.kind = "on" /\ dlg.phase = "form" /\ ~dlg.blocked
  /\ dlg.pre.phase # "running" /\ dlg.chosen # {}
  /\ \E dry \in BOOLEAN :
       calls' = [calls EXCEPT ![Free] =
         [NoCall EXCEPT !.kind = "install", !.phase = "sent",
                        !.seen = IF Mut("SendsTheScreenBasis") THEN scr ELSE dlg.shown,
                        !.shown = dlg.shown, !.roots = dlg.chosen, !.dry = dry,
                        !.warned = TRUE, !.own = TRUE, !.page = TRUE]]
  /\ dlg' = [dlg EXCEPT !.phase = "saving"]
  /\ UNCHANGED <<svc, ver, run, scr, last, n>>

\* The switch when the screen shows the service on: the turn-off dialog.
YouOpenTurnOff ==
  /\ dlg.kind = "none" /\ NoneOwn /\ scr.on
  /\ n.opens < MaxOpens
  /\ n' = [n EXCEPT !.opens = @ + 1]
  /\ dlg' = [NoDialog EXCEPT !.kind = "off", !.phase = "form"]
  /\ UNCHANGED <<svc, ver, run, scr, calls, last>>

YouTurnOff ==
  /\ dlg.kind = "off" /\ dlg.phase = "form"
  /\ calls' = [calls EXCEPT ![Free] =
       [NoCall EXCEPT !.kind = "uninstall", !.phase = "sent", !.own = TRUE, !.page = TRUE]]
  /\ dlg' = [dlg EXCEPT !.phase = "saving"]
  /\ UNCHANGED <<svc, ver, run, scr, last, n>>

\* Cancel, Escape or a click outside, disabled while Preview or a call runs.
\* Closing the save dialog reads the screen again when the dialog read differs.
YouCancel ==
  /\ dlg.kind # "none" /\ dlg.phase \in {"reading", "form"}
  /\ dlg.pre.phase # "running"
  /\ dlg' = NoDialog
  /\ UNCHANGED <<svc, ver, run, scr, calls, last, n>>

\* Command-1 to 5, the sidebar, or another call of `go` in App.tsx: another
\* screen, then back. Leaving unmounts the dialog; a call it made runs on in
\* src-tauri, and its reply reaches no dialog. `go` refuses while a save or
\* turn-off of this window runs (`changingService`); with "LeaveWhileSaving"
\* it does not.
YouLeave ==
  /\ dlg.kind # "none"
  /\ ~Held \/ Mut("LeaveWhileSaving")
  /\ dlg' = NoDialog
  /\ calls' = [i \in Slots |-> [calls[i] EXCEPT !.own = FALSE]]
  /\ UNCHANGED <<svc, ver, run, scr, last, n>>

-----------------------------------------------------------------------------
(* service.rs `install`: take `Changes`, read again, compare, check, then  *)
(* install with `--expect`. `uninstall`: take `Changes`, then uninstall.   *)

\* `Changes::start`, then the fresh read against the basis the window sent.
\* Another call runs: refused, nothing run. Equal: go on, unless the plan is
\* blocked. Different: `Save::Changed`, nothing installed.
InstallChecks(i) ==
  /\ calls[i].kind = "install" /\ calls[i].phase = "sent"
  /\ LET c == calls[i]
         fresh == Read
         differs ==
           IF Mut("NoBasisCheck") THEN FALSE
           ELSE IF Mut("BasisWithoutMode")
             THEN [fresh EXCEPT !.dry = FALSE] # [c.seen EXCEPT !.dry = FALSE]
           ELSE IF Mut("BasisWithoutVersion")
             THEN [fresh EXCEPT !.ver = "none", !.fp = "none"] # [c.seen EXCEPT !.ver = "none", !.fp = "none"]
           ELSE fresh # c.seen
     IN calls' = [calls EXCEPT ![i] =
                    IF Refused(i)
                      THEN [c EXCEPT !.phase = "failed"]
                    ELSE IF differs
                      THEN [c EXCEPT !.phase = "changed", !.fresh = fresh, !.freshBlocked = Blocked]
                    ELSE IF Blocked /\ ~Mut("NoBlockedCheck")
                      THEN [c EXCEPT !.phase = "failed"]
                    ELSE [c EXCEPT !.phase = "checked", !.keep = Keep(fresh), !.ver = fresh.ver]]
  /\ UNCHANGED <<svc, ver, run, scr, dlg, last, n>>

\* With Copy, `install` writes the app's own copy from the bundled residue
\* before residue runs (`copy_sidecar`), and keeps the copy it replaces
\* under a second name (`Previous::keep`). It copies only the residue the
\* save planned with: a bundled residue that is now another release is
\* refused, with nothing copied (`copy_sidecar` runs the new copy's
\* `--version` before it renames it into place). With "NoCopyCheck" it
\* copies whatever is there.
InstallCopies(i) ==
  /\ Copy
  /\ calls[i].kind = "install" /\ calls[i].phase = "checked"
  /\ LET c == calls[i] IN
     IF ver # c.ver /\ ~Mut("NoCopyCheck")
       THEN /\ calls' = [calls EXCEPT ![i].phase = "failed"]
            /\ UNCHANGED run
       ELSE /\ calls' = [calls EXCEPT ![i] = [c EXCEPT !.phase = "copied", !.copied = TRUE,
                                                       !.prev = run]]
            /\ run' = ver
  /\ UNCHANGED <<svc, ver, scr, dlg, last, n>>

\* The copy after a save that wrote nothing: the kept one, put back
\* (`Previous::restore`). With "NoPutBack" the new one stays.
PutBack(c) == IF c.copied /\ ~Mut("NoPutBack") THEN c.prev ELSE run

\* `residue service install --expect` with the fingerprint of the basis the
\* window sent. Under its lock, which every `residue service install` and
\* `uninstall` takes, residue compares the service and its own release with
\* that fingerprint right before it writes, so the two are one step. They
\* differ: exit 4, nothing written, the copy put back, and `install` reads
\* the service again for `Save::Changed`. Equal: residue writes the
\* service, the folders and mode of the click, the options the fresh read
\* keeps, run by the residue it found, with Copy the one in the app's copy.
\* The reply can still fail: the read after the install, or a deadline.
\* With "NoExpect" it writes whatever runs.
InstallLands(i) ==
  /\ calls[i].kind = "install" /\ calls[i].phase = IF Copy THEN "copied" ELSE "checked"
  /\ LET c == calls[i] IN
     IF Now # Fp(c.seen) /\ ~Mut("NoExpect")
       THEN /\ run' = PutBack(c)
            /\ calls' = [calls EXCEPT ![i] = [c EXCEPT !.phase = "changed", !.fresh = Read,
                                                       !.freshBlocked = Blocked,
                                                       !.after = IF c.copied THEN run' ELSE c.after]]
            /\ UNCHANGED <<svc, last>>
       ELSE /\ svc' = [on |-> TRUE, roots |-> c.roots, dry |-> c.dry, opts |-> c.keep, odd |-> FALSE]
            /\ last' = [set |-> TRUE, before |-> svc, after |-> svc', ver |-> IF Copy THEN run ELSE ver,
                        shown |-> c.shown, roots |-> c.roots, dry |-> c.dry]
            /\ \E ok \in BOOLEAN :
                 calls' = [calls EXCEPT ![i] = [c EXCEPT !.phase = IF ok THEN "landed" ELSE "failed",
                                                         !.wrote = TRUE]]
            /\ UNCHANGED run
  /\ UNCHANGED <<ver, scr, dlg, n>>

\* The read fails, the copy fails, or `residue service install` fails
\* before it writes: the copy is put back.
InstallFails(i) ==
  /\ calls[i].kind = "install" /\ calls[i].phase \in {"sent", "checked", "copied"}
  /\ LET c == calls[i] IN
     /\ run' = PutBack(c)
     /\ calls' = [calls EXCEPT ![i] = [c EXCEPT !.phase = "failed",
                                               !.after = IF c.copied THEN run' ELSE c.after]]
  /\ UNCHANGED <<svc, ver, scr, dlg, last, n>>

\* `Changes::start` in `uninstall`: refused while another call runs.
UninstallStarts(i) ==
  /\ calls[i].kind = "uninstall" /\ calls[i].phase = "sent"
  /\ calls' = [calls EXCEPT ![i].phase = IF Refused(i) THEN "failed" ELSE "running"]
  /\ UNCHANGED <<svc, ver, run, scr, dlg, last, n>>

\* `residue service uninstall`, with no basis: turning off makes nothing delete.
UninstallLands(i) ==
  /\ calls[i].kind = "uninstall" /\ calls[i].phase = "running"
  /\ svc' = Off
  /\ \E ok \in BOOLEAN :
       calls' = [calls EXCEPT ![i] = [calls[i] EXCEPT !.phase = IF ok THEN "landed" ELSE "failed",
                                                      !.wrote = TRUE]]
  /\ UNCHANGED <<ver, run, scr, dlg, last, n>>

UninstallFails(i) ==
  /\ calls[i].kind = "uninstall" /\ calls[i].phase \in {"sent", "running"}
  /\ calls' = [calls EXCEPT ![i].phase = "failed"]
  /\ UNCHANGED <<svc, ver, run, scr, dlg, last, n>>

\* The reply settles the call, and `changingService` lets it go. When its
\* dialog is open, the reply reaches it, and it cannot close while the call
\* runs. Saved or turned off: the dialog closes and the screen reads again.
\* Changed: the form plans from the fresh read it returned, ticks its folders
\* where you had left them as the dialog started, hides the preview, says
\* nothing was saved, and saves only on another click. Failed: the form shows
\* the error, and a failed save reads the screen again. A reply whose dialog
\* is gone changes nothing on screen.
ReplyArrives(i) ==
  /\ calls[i].phase \in {"landed", "changed", "failed"}
  /\ LET c == calls[i] IN
     IF c.phase = "changed" /\ c.own /\ Mut("RetryOnChange")
       THEN /\ calls' = [calls EXCEPT ![i] = [c EXCEPT !.phase = "sent", !.seen = c.fresh]]
            /\ UNCHANGED <<dlg, scr>>
     ELSE IF ~c.own
       THEN /\ calls' = [calls EXCEPT ![i] = NoCall]
            /\ UNCHANGED <<dlg, scr>>
     ELSE /\ calls' = [calls EXCEPT ![i] = NoCall]
          /\ dlg' = CASE c.phase = "landed" \/ dlg.kind = "none" -> NoDialog
                      [] c.phase = "changed" ->
                           [dlg EXCEPT !.phase = "form", !.shown = c.fresh,
                                       !.blocked = c.freshBlocked, !.pre = NoPreview,
                                       !.chosen = IF @ = StartRoots(dlg.shown)
                                                    THEN StartRoots(c.fresh) ELSE @]
                      [] OTHER -> [dlg EXCEPT !.phase = "form"]
          /\ scr' = IF c.phase = "landed" \/ (c.phase = "failed" /\ c.kind = "install")
                      THEN Read ELSE scr
  /\ UNCHANGED <<svc, ver, run, last, n>>

-----------------------------------------------------------------------------
(* Faults, at most one in a behavior.                                       *)

\* The window starts afresh while a call of the app runs on in src-tauri: the
\* page loads again, or the last-resort "Try again" in src/main.tsx draws App
\* anew. No dialog is open, and no call counts in the new window's
\* `changingService` (a "Try again" keeps it, so this covers more than it does).
Reload ==
  /\ "reload" \in Faults /\ n.faults = 0
  /\ ~Idle
  /\ dlg' = NoDialog
  /\ calls' = [i \in Slots |-> [calls[i] EXCEPT !.own = FALSE, !.page = FALSE]]
  /\ n' = [n EXCEPT !.faults = 1]
  /\ UNCHANGED <<svc, ver, run, scr, last>>

-----------------------------------------------------------------------------
(* Outside the app: a terminal or `brew upgrade`, while no save of the app  *)
(* runs, or with DuringSave at any time.                                    *)

Outside(kind, new, v) ==
  /\ kind \in World
  /\ Idle \/ DuringSave
  /\ n.outside < MaxOutside
  /\ svc' = new /\ ver' = v
  /\ n' = [n EXCEPT !.outside = @ + 1]
  /\ UNCHANGED <<run, scr, dlg, calls, last>>

TerminalTurnsOn ==
  /\ ~svc.on
  /\ Outside("on", [on |-> TRUE, roots |-> Folders, dry |-> FALSE, opts |-> "set", odd |-> FALSE], ver)

TerminalTurnsOff == svc.on /\ Outside("off", Off, ver)

TerminalChangesFolders ==
  /\ svc.on
  /\ \E roots \in (SUBSET Folders) \ {{}, svc.roots} : Outside("folders", [svc EXCEPT !.roots = roots], ver)

TerminalSwitchesMode == svc.on /\ Outside("mode", [svc EXCEPT !.dry = ~@], ver)

TerminalChangesOptions ==
  /\ svc.on
  /\ Outside("options", [svc EXCEPT !.opts = IF @ = "set" THEN "defaults" ELSE "set"], ver)

\* An argument the app cannot read, such as an option a newer residue adds.
TerminalAddsArgument == svc.on /\ ~svc.odd /\ Outside("argument", [svc EXCEPT !.odd = TRUE], ver)

\* A newer residue at the binary a save installs with, and the service runs.
BrewUpgrade == ~Copy /\ ver = "old" /\ Outside("brew", svc, "new")

\* A newer or older app put in place of this one while it runs: the bundled
\* residue is another release, at the same path.
AppReplaced == Copy /\ Outside("app", svc, IF ver = "old" THEN "new" ELSE "old")

\* Nothing is left to happen: no dialog and no call.
Finished ==
  /\ dlg.kind = "none" /\ Idle
  /\ UNCHANGED vars

Next ==
  \/ ScreenReads \/ YouOpenSave \/ DialogReads \/ YouTick
  \/ YouPreview \/ PreviewLands \/ PreviewFails \/ YouSave
  \/ YouOpenTurnOff \/ YouTurnOff \/ YouCancel \/ YouLeave
  \/ \E i \in Slots : InstallChecks(i) \/ InstallCopies(i) \/ InstallLands(i) \/ InstallFails(i)
  \/ \E i \in Slots : UninstallStarts(i) \/ UninstallLands(i) \/ UninstallFails(i)
  \/ \E i \in Slots : ReplyArrives(i)
  \/ Reload
  \/ TerminalTurnsOn \/ TerminalTurnsOff \/ TerminalChangesFolders
  \/ TerminalSwitchesMode \/ TerminalChangesOptions \/ TerminalAddsArgument
  \/ BrewUpgrade \/ AppReplaced
  \/ Finished

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* The rules. `last` is the app's last install.                             *)

Service(s) == [on |-> s.on, roots |-> s.roots, dry |-> s.dry, opts |-> s.opts]

\* A save installs the folders and mode you chose, with the options and the
\* residue the dialog showed.
SaveIsWhatTheDialogShowed ==
  last.set =>
    /\ last.after.roots = last.roots /\ last.after.dry = last.dry
    /\ last.after.opts = Keep(last.shown)
    /\ last.ver = last.shown.ver

\* A save replaces only the service the dialog showed.
ReplacesOnlyWhatItShowed ==
  last.set => Service(last.before) = Service(last.shown)

\* A save never drops an argument the app cannot read.
NeverDropsAnArgument == last.set => ~last.before.odd

\* A save that installs nothing leaves the app's own copy, and so the
\* residue the service runs, as it was before the save.
FailedSaveKeepsTheResidue ==
  \A i \in Slots :
    calls[i].copied /\ calls[i].phase \in {"failed", "changed"} /\ ~calls[i].wrote
      => calls[i].after = calls[i].prev

\* Every save comes from a dialog that said the service deletes instead of
\* using the trash.
WarnedBeforeItDeletes == \A i \in Slots : calls[i].kind = "install" => calls[i].warned

\* The preview the dialog shows ran on exactly the folders it shows ticked.
PreviewIsOfTheTickedFolders ==
  dlg.kind = "on" /\ dlg.pre.phase = "shown" => dlg.pre.roots = dlg.chosen

\* While the dialog saves, it shows ticked exactly the folders the save installs.
SaveIsOfTheTickedFolders ==
  \A i \in Slots :
    dlg.kind = "on" /\ calls[i].own /\ calls[i].kind = "install" => calls[i].roots = dlg.chosen

\* A save or turn-off keeps its dialog open until its reply arrives.
TheDialogStaysWhileItSaves == \A i \in Slots : calls[i].page => calls[i].own

\* At most one `install` or `uninstall` of the app runs at a time.
OneChangeAtATime == Cardinality({i \in Slots : Runs(i)}) <= 1

\* SaveChart.cfg's bound, to keep the chart under its budget of 500 states:
\* the save dialog with a folder ticked and no preview. Unticking every
\* folder only disables Preview and Save, and a preview stays in the state
\* "Shows the service as it is". check.sh sets no bound.
ChartBound ==
  /\ dlg.kind = "on" /\ dlg.phase # "reading" => dlg.chosen # {}
  /\ dlg.pre.phase = "none"

=============================================================================
