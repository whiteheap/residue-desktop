----------------------------- MODULE RemoveFlow -----------------------------
(***************************************************************************)
(* How the app moves worktrees to the trash, as the window and residue     *)
(* take turns: the Worktrees screen and a worktree's checks, the removal   *)
(* dialog (src/views/RemoveDialog.tsx), and the two runs of                *)
(* `residue remove --trash --json PATH...` it starts through               *)
(* src-tauri/src/lib.rs: the plan without --yes, and after the confirm the *)
(* same command with --yes. Each reply can come late, fail, or find the    *)
(* window gone. The coverage table in ../SKILL.md maps each action to the  *)
(* code.                                                                   *)
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
  win,    \* how many times the window started afresh
  n,      \* counters: dialog opens, changes, ids, faults
  wrong   \* worktrees a run moved that the list it was confirmed from did not show

vars == <<truth, shown, scan, sel, det, dlg, plans, runs, win, n, wrong>>

Phases == {"none", "planning", "plan", "running", "done", "error", "broken"}

\* The dialog when it is not open. `id` names one opening of the dialog (one
\* mount of RemoveDialog); `req` is the plan request its effect waits for.
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
  /\ win = 0
  /\ n = [opens |-> 0, changes |-> 0, id |-> 0, faults |-> 0]
  /\ wrong = {}

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
  /\ UNCHANGED <<shown, scan, sel, det, dlg, plans, runs, win, wrong>>

\* Rescan or Command-R. Only the latest scan lands (useCli).
YouRescan ==
  /\ ~dlg.open /\ ~scan
  /\ scan' = TRUE
  /\ UNCHANGED <<truth, shown, sel, det, dlg, plans, runs, win, n, wrong>>

ScanLands ==
  /\ scan
  /\ scan' = FALSE
  /\ shown' = [w \in W |-> Verdict(w)]
  /\ UNCHANGED <<truth, sel, det, dlg, plans, runs, win, n, wrong>>

\* A row has a checkbox only for a worktree that can go. The list, its
\* checkboxes and its action bar show while no worktree's checks are open.
YouTick(w) ==
  /\ ~dlg.open \/ Mut("NotModal")
  /\ det.path = "none"
  /\ w \notin sel
  /\ shown[w] = "go"
  /\ sel' = sel \cup {w}
  /\ UNCHANGED <<truth, shown, scan, det, dlg, plans, runs, win, n, wrong>>

\* What the action bar moves: the ticked worktrees the shown scan says can go
\* (Worktrees.tsx `chosen`). A tick stays when a later scan says keep.
Chosen == IF Mut("ChosenUnfiltered") THEN sel ELSE {w \in sel : shown[w] = "go"}

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
  /\ Chosen # {} /\ det.path = "none"
  /\ IF dlg.open
       THEN /\ Mut("NotModal")
            /\ Chosen # dlg.paths
            /\ dlg' = [dlg EXCEPT !.paths = Chosen, !.req = n.id + 1,
                                  !.ok = @ /\ \A w \in Chosen : shown[w] = "go"]
            /\ plans' = plans \cup {[id |-> n.id + 1, dlg |-> dlg.id, paths |-> Chosen]}
            /\ n' = [n EXCEPT !.id = @ + 1]
       ELSE Open(Chosen, \A w \in Chosen : shown[w] = "go")
  /\ UNCHANGED <<truth, shown, scan, sel, det, runs, win, wrong>>

\* Open a worktree's checks (`residue explain`), read once when they open.
YouOpenChecks(w) ==
  /\ ~dlg.open /\ det.path = "none"
  /\ shown[w] # "none"
  /\ det' = [path |-> w, v |-> "loading"]
  /\ UNCHANGED <<truth, shown, scan, sel, dlg, plans, runs, win, n, wrong>>

ChecksLand ==
  /\ det.v = "loading"
  /\ det' = [det EXCEPT !.v = Verdict(det.path)]
  /\ UNCHANGED <<truth, shown, scan, sel, dlg, plans, runs, win, n, wrong>>

\* Back, or Escape: the list again.
YouLeaveChecks ==
  /\ ~dlg.open /\ det.path # "none"
  /\ det' = NoChecks
  /\ UNCHANGED <<truth, shown, scan, sel, dlg, plans, runs, win, n, wrong>>

\* "Move this worktree to the trash...", shown only for a worktree that can go.
YouClickMoveInChecks ==
  /\ ~dlg.open
  /\ IF Mut("ChecksButtonForEveryVerdict") THEN det.v \in {"go", "keep"} ELSE det.v = "go"
  /\ Open({det.path}, det.v = "go")
  /\ UNCHANGED <<truth, shown, scan, sel, det, runs, win, wrong>>

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
  /\ UNCHANGED <<shown, scan, sel, det, runs, win, n>>

PlanFails == \E p \in plans :
  /\ plans' = plans \ {p}
  /\ dlg' = IF dlg.open /\ dlg.req = p.id /\ dlg.phase # "broken"
              THEN [dlg EXCEPT !.phase = "error", !.withPlan = FALSE]
              ELSE dlg
  /\ UNCHANGED <<truth, shown, scan, sel, det, runs, win, n, wrong>>

\* "Move N worktrees to the trash": the confirm button, disabled while a run
\* runs. It sends the paths of the list it shows.
YouConfirm ==
  /\ dlg.open
  /\ dlg.ready # {}
  /\ \/ dlg.phase = "plan"
     \/ Mut("ConfirmTwice") /\ dlg.phase = "running" /\ Cardinality(runs) < 2
  /\ runs' = runs \cup {[id |-> n.id + 1, dlg |-> dlg.id, win |-> win,
                         args |-> IF Mut("ConfirmSendsChoice") THEN dlg.paths ELSE dlg.ready,
                         listed |-> dlg.ready, st |-> "sent", moved |-> {}]}
  /\ dlg' = [dlg EXCEPT !.phase = "running"]
  /\ n' = [n EXCEPT !.id = @ + 1]
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, win, wrong>>

\* residue checks every path once more and moves the ones that can still go.
RunMoves == \E r \in runs :
  /\ r.st = "sent"
  /\ LET mv == Moves(r.args) IN
     /\ truth' = [w \in W |-> IF w \in mv THEN "trash" ELSE truth[w]]
     /\ wrong' = wrong \cup (mv \ r.listed)
     /\ runs' = (runs \ {r}) \cup {[r EXCEPT !.st = "moved", !.moved = mv]}
     /\ dlg' = IF dlg.id = r.dlg THEN [dlg EXCEPT !.moved = @ \cup mv] ELSE dlg
  /\ UNCHANGED <<shown, scan, sel, det, plans, win, n>>

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
  /\ UNCHANGED <<truth, shown, plans, win, n, wrong>>

\* residue exits with an error or no report, before it moved anything or
\* after (the 30-minute deadline, a document the app cannot read). The
\* dialog keeps the plan and says to scan again to see what moved.
RunFails == \E r \in runs :
  /\ runs' = runs \ {r}
  /\ dlg' = IF dlg.id = r.dlg /\ dlg.phase # "broken"
              THEN [dlg EXCEPT !.phase = "error", !.withPlan = ~Mut("RunErrorSaysNothingMoved")]
              ELSE dlg
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, win, n, wrong>>

\* Cancel, Close, Escape or a click outside. None of them closes the dialog
\* while it plans or runs (`busy`).
YouClose ==
  /\ dlg.open
  /\ \/ dlg.phase \in {"plan", "done", "error"}
     \/ Mut("CloseWhileBusy") /\ dlg.phase \in {"planning", "running"}
  /\ dlg' = Closed
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, runs, win, n, wrong>>

-----------------------------------------------------------------------------
(* Faults, at most one in a behavior.                                       *)

\* The dialog fails to draw. Its ErrorBoundary shows a panel whose one way
\* out is Close; Close leaves the dialog and scans again. A reply that lands
\* after that changes nothing in the window but App's onRemoved.
DrawFails ==
  /\ "draw" \in Faults /\ n.faults = 0
  /\ dlg.open /\ dlg.phase # "broken"
  /\ dlg' = [dlg EXCEPT !.phase = "broken"]
  /\ n' = [n EXCEPT !.faults = 1]
  /\ UNCHANGED <<truth, shown, scan, sel, det, plans, runs, win, wrong>>

YouCloseBrokenDialog ==
  /\ dlg.open /\ dlg.phase = "broken"
  /\ dlg' = Closed
  /\ scan' = TRUE
  /\ UNCHANGED <<truth, shown, sel, det, plans, runs, win, n, wrong>>

\* The window starts afresh (the last-resort "Try again", which draws App
\* anew, or a quit and a relaunch) while residue keeps running: no dialog,
\* nothing ticked, and a first scan. Replies to the old window are dropped.
Reload ==
  /\ "reload" \in Faults /\ n.faults = 0
  /\ win' = win + 1
  /\ dlg' = Closed
  /\ sel' = {}
  /\ det' = NoChecks
  /\ shown' = [w \in W |-> "none"]
  /\ scan' = TRUE
  /\ n' = [n EXCEPT !.faults = 1]
  /\ UNCHANGED <<truth, plans, runs, wrong>>

\* Nothing is left to happen: the dialog is closed and residue has replied.
Finished ==
  /\ ~dlg.open /\ plans = {} /\ runs = {} /\ ~scan
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
  \/ YouClose
  \/ DrawFails \/ YouCloseBrokenDialog \/ Reload
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
