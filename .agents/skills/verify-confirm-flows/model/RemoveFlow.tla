----------------------------- MODULE RemoveFlow -----------------------------
(***************************************************************************)
(* How the app moves worktrees to the trash, as the window and residue     *)
(* take turns: the Worktrees screen and a worktree's checks, the removal   *)
(* dialog (src/views/RemoveDialog.tsx), and the two runs of                *)
(* `residue remove --trash --json PATH...` it starts through               *)
(* src-tauri/src/lib.rs: the plan without --yes, and after the confirm the *)
(* same command with --yes. Each reply can come late, fail, or find the    *)
(* window gone. src-tauri knows when a confirmed run is in flight, also    *)
(* one an earlier launch of the app started, and the window asks it        *)
(* (src-tauri/src/removal.rs, #12). The coverage table in ../SKILL.md maps *)
(* each action to the code.                                                *)
(*                                                                         *)
(* A worktree is "go" when residue would move it now (verdict candidate),  *)
(* "keep" when it would keep it, and "trash" once a run moved it.          *)
(***************************************************************************)
EXTENDS FiniteSets, Naturals

CONSTANTS
  W,          \* the worktrees, such as {"a", "b"}
  Faults,     \* what may go wrong once in a behavior: "reload", "draw"
  Broken,     \* the guards removed, from model/mutants; {} is the code
  MaxOpens,   \* how many times the dialog opens
  MaxChanges  \* how many times a worktree turns from go to keep or back

Mut(name) == name \in Broken

VARIABLES
  truth,  \* what residue decides for each worktree now
  shown,  \* the verdict the Worktrees screen shows: "go", "keep" or "none"
  scan,   \* a scan runs; when it lands it replaces `shown`
  sel,    \* the worktrees ticked on the Worktrees screen (App `selected`)
  det,    \* the worktree whose checks are open, and the verdict they show
  dlg,    \* the removal dialog
  plans,  \* plan runs (without --yes) that have not replied
  runs,   \* confirmed runs (--yes) that have not replied
  heard,  \* whether a confirmed run is in flight, as the window last heard
          \* from src-tauri (App `inFlight`): "asking", "none" or "running"
  win,    \* how many times the window started afresh
  app,    \* how many times the app quit and opened again: its process
  n,      \* counters: dialog opens, changes, ids, faults
  wrong   \* worktrees a run moved that the list it was confirmed from did not show

vars == <<truth, shown, scan, sel, det, dlg, plans, runs, heard, win, app, n, wrong>>

Phases == {"none", "planning", "plan", "running", "done", "error", "broken"}

\* The dialog when it is not open. `id` names one opening of the dialog (one
\* mount of RemoveDialog); `req` is the plan request its effect waits for.
\* In "error", `withPlan` is the text that says to scan again to see what
\* moved; without it the dialog says that nothing moved.
Closed == [open |-> FALSE, id |-> 0, paths |-> {}, req |-> 0, phase |-> "none",
           ready |-> {}, planFor |-> {}, withPlan |-> FALSE, ok |-> TRUE,
           moved |-> {}]

NoChecks == [path |-> "none", v |-> "none"]

Verdict(w) == IF truth[w] = "trash" THEN "none" ELSE truth[w]

TypeOK ==
  /\ truth \in [W -> {"go", "keep", "trash"}]
  /\ shown \in [W -> {"go", "keep", "none"}]
  /\ scan \in BOOLEAN
  /\ sel \subseteq W
  /\ det \in [path : W \cup {"none"}, v : {"loading", "go", "keep", "none"}]
  /\ dlg.open \in BOOLEAN /\ dlg.phase \in Phases
  /\ dlg.paths \subseteq W /\ dlg.ready \subseteq W /\ dlg.planFor \subseteq W
  /\ dlg.moved \subseteq W /\ dlg.ok \in BOOLEAN /\ dlg.withPlan \in BOOLEAN
  /\ \A p \in plans : p.paths \subseteq W
  /\ \A r \in runs : r.args \subseteq W /\ r.listed \subseteq W
                     /\ r.st \in {"sent", "moved"} /\ r.moved \subseteq W
                     /\ r.app \in Nat
  /\ heard \in {"asking", "none", "running"}
  /\ app \in Nat
  /\ wrong \subseteq W

Init ==
  /\ truth \in [W -> {"go", "keep"}]
  /\ shown = truth
  /\ scan = FALSE
  /\ sel = {}
  /\ det = NoChecks
  /\ dlg = Closed
  /\ plans = {}
  /\ runs = {}
  \* App's first ask: nothing is in flight yet, so src-tauri answers "none".
  \* Starting there leaves out only steps taken while Move to the trash is
  \* still off.
  /\ heard = "none"
  /\ win = 0
  /\ app = 0
  /\ n = [opens |-> 0, changes |-> 0, id |-> 0, faults |-> 0]
  /\ wrong = {}

\* src-tauri knows that a confirmed run is in flight (removal.rs,
\* `Removals`): one this app started, held in its State from before residue
\* starts until `remove` returns ("NoRunGuard" drops it), or one an earlier
\* launch of the app started, from the marker naming residue's process that
\* the app reads when it opens, until that process ends ("NoMarker" drops
\* it).
InFlight == \E r \in runs : IF r.app = app THEN ~Mut("NoRunGuard") ELSE ~Mut("NoMarker")

-----------------------------------------------------------------------------
(* The Worktrees screen and a worktree's checks. While the dialog is open   *)
(* it is modal and App ignores Command keys (`if (removing) return`), so     *)
(* nothing outside it can be clicked.                                       *)

\* Outside the app a worktree changes: a commit, a branch that lands.
Change(w) ==
  /\ n.changes < MaxChanges
  /\ truth[w] # "trash"
  /\ truth' = [truth EXCEPT ![w] = IF @ = "go" THEN "keep" ELSE "go"]
  /\ n' = [n EXCEPT !.changes = @ + 1]
  /\ UNCHANGED <<shown, scan, sel, det, dlg, plans, runs, heard, win, app, wrong>>

\* Rescan or Command-R. Only the latest scan lands (useCli).
YouRescan ==
  /\ ~dlg.open /\ ~scan
  /\ scan' = TRUE
  /\ UNCHANGED <<truth, shown, sel, det, dlg, plans, runs, heard, win, app, n, wrong>>

ScanLands ==
  /\ scan
  /\ scan' = FALSE
  /\ shown' = [w \in W |-> Verdict(w)]
  /\ UNCHANGED <<truth, sel, det, dlg, plans, runs, heard, win, app, n, wrong>>

\* A row has a checkbox only for a worktree that can go. The list, its
\* checkboxes and its action bar show while no worktree's checks are open.
YouTick(w) ==
  /\ ~dlg.open \/ Mut("NotModal")
  /\ det.path = "none"
  /\ w \notin sel
  /\ shown[w] = "go"
  /\ sel' = sel \cup {w}
  /\ UNCHANGED <<truth, shown, scan, det, dlg, plans, runs, heard, win, app, n, wrong>>

\* What the action bar moves: the ticked worktrees the shown scan says can go
\* (Worktrees.tsx `chosen`). A tick stays when a later scan says keep.
Chosen == IF Mut("ChosenUnfiltered") THEN sel ELSE {w \in sel : shown[w] = "go"}

\* Both buttons that open the dialog are on only once the window heard that
\* no confirmed run is in flight (`inFlight` "none"). src-tauri's refusal in
\* YouConfirm backs this, so it has no mutant (../SKILL.md).
MoveOn == heard = "none"

\* Open the dialog for `paths`, which the window showed as can go when `ok`.
\* Its effect starts the plan.
Open(paths, ok) ==
  /\ n.opens < MaxOpens
  /\ dlg' = [open |-> TRUE, id |-> n.id + 1, paths |-> paths, req |-> n.id + 1,
             phase |-> "planning", ready |-> {}, planFor |-> {},
             withPlan |-> FALSE, ok |-> ok, moved |-> {}]
  /\ plans' = plans \cup {[id |-> n.id + 1, dlg |-> n.id + 1, paths |-> paths]}
  /\ n' = [n EXCEPT !.opens = @ + 1, !.id = @ + 1]

\* "Move N to the trash..." on the action bar. With "NotModal" the bar can be
\* clicked while the dialog is open: the dialog gets other paths, keeps its
\* phase, and its effect asks for a new plan.
YouClickMove ==
  /\ MoveOn
  /\ Chosen # {} /\ det.path = "none"
  /\ IF dlg.open
       THEN /\ Mut("NotModal")
            /\ Chosen # dlg.paths
            /\ dlg' = [dlg EXCEPT !.paths = Chosen, !.req = n.id + 1,
                                  !.ok = @ /\ \A w \in Chosen : shown[w] = "go"]
            /\ plans' = plans \cup {[id |-> n.id + 1, dlg |-> dlg.id, paths |-> Chosen]}
            /\ n' = [n EXCEPT !.id = @ + 1]
       ELSE Open(Chosen, \A w \in Chosen : shown[w] = "go")
  /\ UNCHANGED <<truth, shown, scan, sel, det, runs, heard, win, app, wrong>>

\* Open a worktree's checks (`residue explain`), read once when they open.
YouOpenChecks(w) ==
  /\ ~dlg.open /\ det.path = "none"
  /\ shown[w] # "none"
  /\ det' = [path |-> w, v |-> "loading"]
  /\ UNCHANGED <<truth, shown, scan, sel, dlg, plans, runs, heard, win, app, n, wrong>>

ChecksLand ==
  /\ det.v = "loading"
  /\ det' = [det EXCEPT !.v = Verdict(det.path)]
  /\ UNCHANGED <<truth, shown, scan, sel, dlg, plans, runs, heard, win, app, n, wrong>>

\* Back, or Escape: the list again.
YouLeaveChecks ==
  /\ ~dlg.open /\ det.path # "none"
  /\ det' = NoChecks
  /\ UNCHANGED <<truth, shown, scan, sel, dlg, plans, runs, heard, win, app, n, wrong>>

\* "Move this worktree to the trash...", shown only for a worktree that can go.
YouClickMoveInChecks ==
  /\ ~dlg.open /\ MoveOn
  /\ IF Mut("ChecksButtonForEveryVerdict") THEN det.v \in {"go", "keep"} ELSE det.v = "go"
  /\ Open({det.path}, det.v = "go")
  /\ UNCHANGED <<truth, shown, scan, sel, det, runs, heard, win, app, wrong>>

-----------------------------------------------------------------------------
(* The dialog and residue's replies. A reply lands only in the dialog whose  *)
(* effect is still waiting for it (`live`), and only while the window that   *)
(* sent it is the same window.                                              *)

Moves(paths) == {w \in paths : truth[w] = "go"}

\* The plan: residue checks each path again and, without --yes, moves
\* nothing. Each worktree it would move is `skipped` with verdict candidate:
\* the list the dialog shows (`ready`). "PlanWithYes" sends --yes.
PlanLands == \E p \in plans :
  /\ plans' = plans \ {p}
  /\ LET mv == IF Mut("PlanWithYes") THEN Moves(p.paths) ELSE {} IN
     /\ truth' = [w \in W |-> IF w \in mv THEN "trash" ELSE truth[w]]
     /\ wrong' = wrong \cup mv
     /\ dlg' = IF dlg.open /\ dlg.req = p.id /\ dlg.phase # "broken"
                 THEN [dlg EXCEPT !.phase = "plan", !.ready = Moves(p.paths) \ mv,
                                  !.planFor = p.paths, !.moved = @ \cup mv]
                 ELSE IF dlg.id = p.dlg THEN [dlg EXCEPT !.moved = @ \cup mv]
                 ELSE dlg
  /\ UNCHANGED <<shown, scan, sel, det, runs, heard, win, app, n>>

PlanFails == \E p \in plans :
  /\ plans' = plans \ {p}
  /\ dlg' = IF dlg.open /\ dlg.req = p.id /\ dlg.phase # "broken"
              THEN [dlg EXCEPT !.phase = "error", !.withPlan = FALSE]
              ELSE dlg
  /\ UNCHANGED <<truth, shown, scan, sel, det, runs, heard, win, app, n, wrong>>

\* "Move N worktrees to the trash": the confirm button, disabled while a run
\* runs. It sends the paths of the list it shows. src-tauri refuses it while
\* a confirmed run is in flight (`Removals::start`), with kind invalid and
\* before residue starts, so the refusal is its reply and one step with it:
\* the dialog says this removal did not start, so nothing moved.
\* "ConfirmWhileBusy" runs it anyway.
YouConfirm ==
  /\ dlg.open
  /\ dlg.ready # {}
  /\ \/ dlg.phase = "plan"
     \/ Mut("ConfirmTwice") /\ dlg.phase = "running" /\ Cardinality(runs) < 2
  /\ IF InFlight /\ ~Mut("ConfirmWhileBusy")
       THEN /\ dlg' = [dlg EXCEPT !.phase = "error", !.withPlan = FALSE]
            /\ UNCHANGED <<runs, n>>
       ELSE /\ runs' = runs \cup {[id |-> n.id + 1, dlg |-> dlg.id, win |-> win, app |-> app,
                                   args |-> IF Mut("ConfirmSendsChoice") THEN dlg.paths ELSE dlg.ready,
                                   listed |-> dlg.ready, st |-> "sent", moved |-> {}]}
            /\ dlg' = [dlg EXCEPT !.phase = "running"]
            /\ n' = [n EXCEPT !.id = @ + 1]
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, heard, win, app, wrong>>

\* residue checks every path once more and moves the ones that can still go.
RunMoves == \E r \in runs :
  /\ r.st = "sent"
  /\ LET mv == Moves(r.args) IN
     /\ truth' = [w \in W |-> IF w \in mv THEN "trash" ELSE truth[w]]
     /\ wrong' = wrong \cup (mv \ r.listed)
     /\ runs' = (runs \ {r}) \cup {[r EXCEPT !.st = "moved", !.moved = mv]}
     /\ dlg' = IF dlg.id = r.dlg THEN [dlg EXCEPT !.moved = @ \cup mv] ELSE dlg
  /\ UNCHANGED <<shown, scan, sel, det, plans, heard, win, app, n>>

\* The report lands: the dialog shows it, and App's onRemoved runs even when
\* the dialog has closed: it unticks what moved, leaves the checks of a
\* worktree that moved, and scans again. A window that started afresh drops it.
RunLands == \E r \in runs :
  /\ r.st = "moved"
  /\ runs' = runs \ {r}
  /\ dlg' = IF dlg.id = r.dlg /\ dlg.phase # "broken"
              THEN [dlg EXCEPT !.phase = "done"] ELSE dlg
  /\ IF r.win = win
       THEN /\ sel' = sel \ r.moved
            /\ det' = IF det.path \in r.moved THEN NoChecks ELSE det
            /\ scan' = TRUE
       ELSE UNCHANGED <<sel, det, scan>>
  /\ UNCHANGED <<truth, shown, plans, heard, win, app, n, wrong>>

\* residue exits with an error or no report, before it moved anything or
\* after (the 30-minute deadline, a document the app cannot read). The
\* dialog keeps the plan and says to scan again to see what moved.
RunFails == \E r \in runs :
  /\ runs' = runs \ {r}
  /\ dlg' = IF dlg.id = r.dlg /\ dlg.phase # "broken"
              THEN [dlg EXCEPT !.phase = "error", !.withPlan = ~Mut("RunErrorSaysNothingMoved")]
              ELSE dlg
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, heard, win, app, n, wrong>>

\* Cancel, Close, Escape or a click outside. None of them closes the dialog
\* while it plans or runs (`busy`). App then asks src-tauri whether a
\* confirmed run is in flight.
YouClose ==
  /\ dlg.open
  /\ \/ dlg.phase \in {"plan", "done", "error"}
     \/ Mut("CloseWhileBusy") /\ dlg.phase \in {"planning", "running"}
  /\ dlg' = Closed
  /\ heard' = "asking"
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, runs, win, app, n, wrong>>

\* src-tauri's answer (`removal_running`), read when it lands. While the
\* window asks, both buttons that open the dialog are off, so no run starts
\* before the answer; a run that ends first can only shorten the wait.
HeardLands ==
  /\ heard = "asking"
  /\ heard' = IF InFlight THEN "running" ELSE "none"
  /\ UNCHANGED <<truth, shown, scan, sel, det, dlg, plans, runs, win, app, n, wrong>>

\* The wait (`wait_for_removal`) returns once no confirmed run is in flight:
\* Move to the trash is on again, and App scans again.
RemovalEnds ==
  /\ heard = "running" /\ ~InFlight
  /\ heard' = "none"
  /\ scan' = TRUE
  /\ UNCHANGED <<truth, shown, sel, det, dlg, plans, runs, win, app, n, wrong>>

\* The ask or the wait fails (App's `.catch`): Move to the trash turns on
\* rather than stay off for good, and src-tauri's refusal is the guard.
HeardFails ==
  /\ heard \in {"asking", "running"}
  /\ heard' = "none"
  /\ UNCHANGED <<truth, shown, scan, sel, det, dlg, plans, runs, win, app, n, wrong>>

-----------------------------------------------------------------------------
(* Faults, at most one in a behavior.                                       *)

\* The dialog fails to draw. Its ErrorBoundary shows a panel whose one way
\* out is Close; Close leaves the dialog, scans again, and asks src-tauri
\* whether a confirmed run is in flight. A reply that lands after that
\* changes nothing in the window but App's onRemoved.
DrawFails ==
  /\ "draw" \in Faults /\ n.faults = 0
  /\ dlg.open /\ dlg.phase # "broken"
  /\ dlg' = [dlg EXCEPT !.phase = "broken"]
  /\ n' = [n EXCEPT !.faults = 1]
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, runs, heard, win, app, wrong>>

YouCloseBrokenDialog ==
  /\ dlg.open /\ dlg.phase = "broken"
  /\ dlg' = Closed
  /\ scan' = TRUE
  /\ heard' = "asking"
  /\ UNCHANGED <<truth, shown, sel, det, plans, runs, win, app, n, wrong>>

\* The window starts afresh while residue keeps running: no dialog, nothing
\* ticked, a first scan, and App asks src-tauri whether a confirmed run is
\* in flight. Replies to the old window are dropped.
Afresh ==
  /\ "reload" \in Faults /\ n.faults = 0
  /\ win' = win + 1
  /\ dlg' = Closed
  /\ sel' = {}
  /\ det' = NoChecks
  /\ shown' = [w \in W |-> "none"]
  /\ scan' = TRUE
  /\ heard' = "asking"
  /\ n' = [n EXCEPT !.faults = 1]
  /\ UNCHANGED <<truth, plans, runs, wrong>>

\* The last-resort "Try again" draws App anew; src-tauri and its State run on.
TryAgain == Afresh /\ UNCHANGED app

\* A quit and a relaunch: a new app process, whose State starts empty, while
\* the residue the old one started can keep running. The new app reads the
\* marker when it opens.
Relaunch == Afresh /\ app' = app + 1

\* Nothing is left to happen: the dialog is closed, residue has replied, and
\* the window heard that no confirmed run is in flight.
Finished ==
  /\ ~dlg.open /\ plans = {} /\ runs = {} /\ ~scan /\ heard = "none"
  /\ UNCHANGED vars

Next ==
  \/ \E w \in W : Change(w)
  \/ \E w \in W : YouTick(w)
  \/ \E w \in W : YouOpenChecks(w)
  \/ YouRescan \/ ScanLands \/ ChecksLand \/ YouLeaveChecks
  \/ YouClickMove \/ YouClickMoveInChecks
  \/ PlanLands \/ PlanFails
  \/ YouConfirm
  \/ RunMoves \/ RunLands \/ RunFails
  \/ YouClose \/ HeardLands \/ RemovalEnds \/ HeardFails
  \/ DrawFails \/ YouCloseBrokenDialog \/ TryAgain \/ Relaunch
  \/ Finished

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* The rules.                                                               *)

\* A worktree goes to the trash only when the list you confirmed showed it.
OnlyListedWorktreesMove == wrong = {}

\* A confirm sends residue exactly the worktrees its list showed.
ConfirmSendsTheList == \A r \in runs : r.args = r.listed

\* The dialog opens only for worktrees the window showed as can go, where you
\* clicked.
OpensOnlyForCanGo == dlg.open => dlg.ok

\* The list in the dialog is residue's plan for the worktrees you chose.
ListIsForYourChoice ==
  dlg.open /\ dlg.phase \in {"plan", "running"} => dlg.planFor = dlg.paths

\* When the dialog says nothing moved, nothing it started moved.
NothingMovedIsTrue ==
  dlg.open /\ dlg.phase = "error" /\ ~dlg.withPlan => dlg.moved = {}

\* At most one confirmed removal runs at a time.
OneRemovalAtATime == Cardinality(runs) <= 1

=============================================================================
