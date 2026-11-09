// The words /statechart uses for RemoveFlow.tla with RemoveChart.cfg (one
// worktree, "a"): each state as the window shows it, each step as what
// happens. The list, a tick and the worktree's checks are one state for each
// verdict, since both buttons that open the dialog read the same verdict.

export const title = "Moving a worktree to the trash";

const dialog = (d) => {
  switch (d.phase) {
    case "planning":
      return "Checking again, nothing moves yet";
    case "plan":
      return d.ready.length > 0 ? "Lists what will move" : "Nothing can go";
    case "running":
      return "Moving";
    case "done":
      return "Done: shows what moved";
    case "error":
      if (d.withPlan) return "Stopped: scan again to see what moved";
      // A confirm src-tauri refused while another removal ran (#12).
      return d.planFor.length > 0 ? "Did not start: nothing moved" : "No plan: nothing moved";
    case "broken":
      return "Could not draw: Close";
    default:
      return "Open";
  }
};

const screen = (v) => {
  if (v.truth.a === "trash") return v.shown.a === "none" ? "In the trash, gone from the list" : "Moved, still listed";
  if (v.shown.a === "none") return "Not listed yet";
  return v.shown.a === "go" ? "Can go" : "Kept";
};

export const state = (v) => (v.dlg.open ? ["Removal dialog", dialog(v.dlg)] : ["Worktrees", screen(v)]);

export const group = (v) => (v.dlg.open ? "dialog" : v.truth.a === "trash" ? "moved" : "list");

export const colors = { list: "teal", dialog: "blue", moved: "orange" };

const labels = {
  Change: () => "it changes outside the app",
  YouTick: () => "you tick it",
  YouOpenChecks: () => "you open its checks",
  ChecksLand: () => "its checks load",
  YouLeaveChecks: () => "you go back",
  YouRescan: () => "you scan again",
  ScanLands: () => "the scan lands",
  YouClickMove: () => "you click Move to the trash",
  YouClickMoveInChecks: () => "you click Move to the trash",
  PlanLands: () => "residue lists what it would move",
  PlanFails: () => "the plan fails",
  YouConfirm: () => "you confirm",
  RunMoves: () => "residue moves what can still go",
  RunLands: () => "residue reports what moved",
  RunFails: () => "the run fails",
  YouClose: () => "you close it",
  HeardLands: () => "the app hears whether a removal runs",
  RemovalEnds: () => "the removal ends",
  HeardFails: () => "the app cannot tell whether a removal runs",
  DrawFails: () => "the dialog fails to draw",
  YouCloseBrokenDialog: () => "you click Close",
  TryAgain: () => "the window starts afresh",
  Relaunch: () => "you quit and open the app again",
};

// With RemoveChart.cfg, ticking, the checks, a scan you start, the app's
// ask whether a removal runs, and the moves while the dialog shows Moving
// stay in one chart state, so the chart does not draw them.
export const step = (action) => (labels[action] ? labels[action]() : null);
