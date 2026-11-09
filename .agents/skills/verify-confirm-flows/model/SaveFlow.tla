------------------------------ MODULE SaveFlow ------------------------------
(***************************************************************************)
(* How the app turns automatic cleanup on, saves other folders, and turns  *)
(* it off, as the window, residue and the world outside take turns: the    *)
(* Automatic cleanup screen and its two dialogs (src/views/Automatic.tsx), *)
(* and `install` and `uninstall` in src-tauri/src/service.rs, which run    *)
(* `residue service install [OPTIONS] [--dry-run] FOLDERS...` and          *)
(* `residue service uninstall`. A terminal or `brew upgrade` can change    *)
(* the service while a dialog is open. The coverage table in ../SKILL.md   *)
(* maps each action to the code.                                           *)
(***************************************************************************)
EXTENDS FiniteSets, Naturals

CONSTANTS
  Folders,     \* the folders a save can watch, such as {"A", "B"}
  AppRoots,    \* the folders the app scans, where a service that is off starts
  Broken,      \* the guards removed, from model/mutants; {} is the code
  MaxOpens,    \* how many times a dialog opens
  MaxOutside,  \* how many changes a terminal or brew makes
  World        \* which of them can happen: "on", "off", "folders", "mode",
               \* "options", "argument", "brew"

Mut(name) == name \in Broken

VARIABLES
  svc,   \* the service launchd runs: on, folders, --dry-run, options, and an
         \* argument the app cannot read (`Program.unknown`)
  ver,   \* the residue a save installs with (`Plan.binary`, `Plan.version`)
  scr,   \* the read the Automatic cleanup screen shows
  dlg,   \* the open dialog: "on" (turn on or save folders), "off", or none
  ins,   \* the app's `service install` or `uninstall` call in flight
  last,  \* what the app's last install replaced and wrote
  n      \* counters: dialog opens, outside changes

vars == <<svc, ver, scr, dlg, ins, last, n>>

Opts == {"defaults", "set"}  \* residue's defaults, or options such as --grace 14d
Off == [on |-> FALSE, roots |-> {}, dry |-> FALSE, opts |-> "defaults", odd |-> FALSE]

\* A read's `Basis`: the service a save replaces, the options it keeps, and
\* the residue it installs with. Only a service that is on has options to keep.
Basis(s, v) == [on |-> s.on, roots |-> s.roots, dry |-> s.dry, opts |-> s.opts, ver |-> v]
Read == Basis(svc, ver)
Keep(b) == IF b.on THEN b.opts ELSE "defaults"
\* `Plan.blocked`: a save would drop an argument the app cannot read.
Blocked == svc.on /\ svc.odd

NoRead == [on |-> FALSE, roots |-> {}, dry |-> FALSE, opts |-> "none", ver |-> "none"]
NoDialog == [kind |-> "none", phase |-> "none", shown |-> NoRead, blocked |-> FALSE]
NoCall == [kind |-> "none", phase |-> "none", seen |-> NoRead, shown |-> NoRead,
           roots |-> {}, dry |-> FALSE, warned |-> FALSE, fresh |-> NoRead,
           freshBlocked |-> FALSE, keep |-> "none", ver |-> "none", wrote |-> FALSE]
NoLast == [set |-> FALSE, before |-> Off, after |-> Off, ver |-> "none", shown |-> NoRead,
           roots |-> {}, dry |-> FALSE]

Bases == [on : BOOLEAN, roots : SUBSET Folders, dry : BOOLEAN,
          opts : Opts \cup {"none"}, ver : {"old", "new", "none"}]

TypeOK ==
  /\ svc \in [on : BOOLEAN, roots : SUBSET Folders, dry : BOOLEAN, opts : Opts, odd : BOOLEAN]
  /\ ver \in {"old", "new"}
  /\ scr \in Bases
  /\ dlg.kind \in {"none", "on", "off"}
  /\ dlg.phase \in {"none", "reading", "form", "saving"}
  /\ dlg.shown \in Bases /\ dlg.blocked \in BOOLEAN
  /\ ins.kind \in {"none", "install", "uninstall"}
  /\ ins.phase \in {"none", "sent", "checked", "landed", "changed", "failed"}
  /\ ins.seen \in Bases /\ ins.shown \in Bases /\ ins.fresh \in Bases
  /\ ins.roots \subseteq Folders /\ ins.dry \in BOOLEAN /\ ins.warned \in BOOLEAN
  /\ ins.wrote \in BOOLEAN

Init ==
  /\ svc \in {Off, [on |-> TRUE, roots |-> AppRoots, dry |-> FALSE, opts |-> "set", odd |-> FALSE]}
  /\ ver = "old"
  /\ scr = Basis(svc, ver)
  /\ dlg = NoDialog
  /\ ins = NoCall
  /\ last = NoLast
  /\ n = [opens |-> 0, outside |-> 0]

-----------------------------------------------------------------------------
(* The screen and the dialogs.                                              *)

\* Refresh, Command-R (also while a dialog is open), or a read after a
\* dialog closes.
ScreenReads ==
  /\ scr # Read
  /\ scr' = Read
  /\ UNCHANGED <<svc, ver, dlg, ins, last, n>>

\* The switch, or "Change folders": the save dialog opens and reads the
\* service itself. With "SwitchInstalls" the switch turns the service on for
\* the app's folders at once, with no dialog.
YouOpenSave ==
  /\ dlg.kind = "none" /\ ins.phase = "none"
  /\ n.opens < MaxOpens
  /\ n' = [n EXCEPT !.opens = @ + 1]
  /\ IF Mut("SwitchInstalls") /\ ~scr.on
       THEN /\ ins' = [NoCall EXCEPT !.kind = "install", !.phase = "sent", !.seen = scr,
                                     !.shown = scr, !.roots = AppRoots, !.warned = FALSE]
            /\ UNCHANGED dlg
       ELSE /\ dlg' = [NoDialog EXCEPT !.kind = "on", !.phase = "reading"]
            /\ UNCHANGED ins
  /\ UNCHANGED <<svc, ver, scr, last>>

\* The dialog's own read lands (`useCli("save-dialog")`). The form says the
\* service deletes instead of using the trash, whatever it shows.
DialogReads ==
  /\ dlg.kind = "on" /\ dlg.phase = "reading"
  /\ dlg' = [dlg EXCEPT !.phase = "form", !.shown = Read, !.blocked = Blocked]
  /\ UNCHANGED <<svc, ver, scr, ins, last, n>>

\* "Save folders" or "Turn on automatic cleanup", with the folders and the
\* mode ticked at the click, any of them. Disabled when the plan is blocked.
\* It sends the basis of the read the dialog shows.
YouSave ==
  /\ dlg.kind = "on" /\ dlg.phase = "form" /\ ~dlg.blocked
  /\ \E roots \in (SUBSET Folders) \ {{}}, dry \in BOOLEAN :
       ins' = [NoCall EXCEPT !.kind = "install", !.phase = "sent",
                             !.seen = IF Mut("SendsTheScreenBasis") THEN scr ELSE dlg.shown,
                             !.shown = dlg.shown, !.roots = roots, !.dry = dry,
                             !.warned = TRUE]
  /\ dlg' = [dlg EXCEPT !.phase = "saving"]
  /\ UNCHANGED <<svc, ver, scr, last, n>>

\* The switch when the screen shows the service on: the turn-off dialog.
YouOpenTurnOff ==
  /\ dlg.kind = "none" /\ ins.phase = "none" /\ scr.on
  /\ n.opens < MaxOpens
  /\ n' = [n EXCEPT !.opens = @ + 1]
  /\ dlg' = [NoDialog EXCEPT !.kind = "off", !.phase = "form"]
  /\ UNCHANGED <<svc, ver, scr, ins, last>>

YouTurnOff ==
  /\ dlg.kind = "off" /\ dlg.phase = "form"
  /\ ins' = [NoCall EXCEPT !.kind = "uninstall", !.phase = "sent"]
  /\ dlg' = [dlg EXCEPT !.phase = "saving"]
  /\ UNCHANGED <<svc, ver, scr, last, n>>

\* Cancel, Escape or a click outside, disabled while a call runs. Closing
\* the save dialog reads the screen again when the dialog read differs.
YouCancel ==
  /\ dlg.kind # "none" /\ dlg.phase \in {"reading", "form"}
  /\ dlg' = NoDialog
  /\ UNCHANGED <<svc, ver, scr, ins, last, n>>

-----------------------------------------------------------------------------
(* service.rs `install`: read again, compare, check, then install.           *)

\* The fresh read against the basis the window sent. Equal: go on, unless
\* the plan is blocked. Different: `Save::Changed`, nothing installed.
InstallChecks ==
  /\ ins.kind = "install" /\ ins.phase = "sent"
  /\ LET fresh == Read
         differs ==
           IF Mut("NoBasisCheck") THEN FALSE
           ELSE IF Mut("BasisWithoutMode")
             THEN [fresh EXCEPT !.dry = FALSE] # [ins.seen EXCEPT !.dry = FALSE]
           ELSE IF Mut("BasisWithoutVersion")
             THEN [fresh EXCEPT !.ver = "none"] # [ins.seen EXCEPT !.ver = "none"]
           ELSE fresh # ins.seen
     IN ins' = IF differs
                 THEN [ins EXCEPT !.phase = "changed", !.fresh = fresh, !.freshBlocked = Blocked]
               ELSE IF Blocked /\ ~Mut("NoBlockedCheck")
                 THEN [ins EXCEPT !.phase = "failed"]
               ELSE [ins EXCEPT !.phase = "checked", !.keep = Keep(fresh), !.ver = ver]
  /\ UNCHANGED <<svc, ver, scr, dlg, last, n>>

\* `residue service install` writes the service: the folders and mode of the
\* click, the options the fresh read keeps, run by the residue it found. The
\* reply can still fail: the read after the install, or a deadline.
InstallLands ==
  /\ ins.kind = "install" /\ ins.phase = "checked"
  /\ svc' = [on |-> TRUE, roots |-> ins.roots, dry |-> ins.dry, opts |-> ins.keep, odd |-> FALSE]
  /\ last' = [set |-> TRUE, before |-> svc, after |-> svc', ver |-> ver, shown |-> ins.shown,
              roots |-> ins.roots, dry |-> ins.dry]
  /\ \E ok \in BOOLEAN : ins' = [ins EXCEPT !.phase = IF ok THEN "landed" ELSE "failed",
                                                 !.wrote = TRUE]
  /\ UNCHANGED <<ver, scr, dlg, n>>

\* The read fails, or `residue service install` fails before it writes.
InstallFails ==
  /\ ins.kind = "install" /\ ins.phase \in {"sent", "checked"}
  /\ ins' = [ins EXCEPT !.phase = "failed"]
  /\ UNCHANGED <<svc, ver, scr, dlg, last, n>>

\* `residue service uninstall`, with no basis: turning off makes nothing delete.
UninstallLands ==
  /\ ins.kind = "uninstall" /\ ins.phase = "sent"
  /\ svc' = Off
  /\ \E ok \in BOOLEAN : ins' = [ins EXCEPT !.phase = IF ok THEN "landed" ELSE "failed",
                                                 !.wrote = TRUE]
  /\ UNCHANGED <<ver, scr, dlg, last, n>>

UninstallFails ==
  /\ ins.kind = "uninstall" /\ ins.phase = "sent"
  /\ ins' = [ins EXCEPT !.phase = "failed"]
  /\ UNCHANGED <<svc, ver, scr, dlg, last, n>>

\* The reply reaches the dialog, which cannot close while the call runs.
\* Saved or turned off: the dialog closes and the screen reads again. Changed:
\* the form plans from the fresh read it returned, says nothing was saved,
\* and saves only on another click. Failed: the form shows the error, and a
\* failed save reads the screen again.
ReplyArrives ==
  /\ ins.phase \in {"landed", "changed", "failed"}
  /\ IF ins.phase = "changed" /\ Mut("RetryOnChange")
       THEN /\ ins' = [ins EXCEPT !.phase = "sent", !.seen = ins.fresh]
            /\ UNCHANGED <<dlg, scr>>
       ELSE /\ ins' = NoCall
            /\ dlg' = CASE ins.phase = "landed" \/ dlg.kind = "none" -> NoDialog
                        [] ins.phase = "changed" ->
                             [dlg EXCEPT !.phase = "form", !.shown = ins.fresh,
                                         !.blocked = ins.freshBlocked]
                        [] OTHER -> [dlg EXCEPT !.phase = "form"]
            /\ scr' = IF ins.phase = "landed" \/ (ins.phase = "failed" /\ ins.kind = "install")
                        THEN Read ELSE scr
  /\ UNCHANGED <<svc, ver, last, n>>

-----------------------------------------------------------------------------
(* Outside the app: a terminal or `brew upgrade`, while no save of the app  *)
(* runs.                                                                    *)

Outside(kind, new, v) ==
  /\ kind \in World
  /\ ins.phase = "none"
  /\ n.outside < MaxOutside
  /\ svc' = new /\ ver' = v
  /\ n' = [n EXCEPT !.outside = @ + 1]
  /\ UNCHANGED <<scr, dlg, ins, last>>

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
BrewUpgrade == ver = "old" /\ Outside("brew", svc, "new")

\* Nothing is left to happen: no dialog and no call.
Finished ==
  /\ dlg.kind = "none" /\ ins.phase = "none"
  /\ UNCHANGED vars

Next ==
  \/ ScreenReads \/ YouOpenSave \/ DialogReads \/ YouSave
  \/ YouOpenTurnOff \/ YouTurnOff \/ YouCancel
  \/ InstallChecks \/ InstallLands \/ InstallFails
  \/ UninstallLands \/ UninstallFails \/ ReplyArrives
  \/ TerminalTurnsOn \/ TerminalTurnsOff \/ TerminalChangesFolders
  \/ TerminalSwitchesMode \/ TerminalChangesOptions \/ TerminalAddsArgument
  \/ BrewUpgrade
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

\* Every save comes from a dialog that said the service deletes instead of
\* using the trash.
WarnedBeforeItDeletes == ins.kind = "install" => ins.warned

=============================================================================
