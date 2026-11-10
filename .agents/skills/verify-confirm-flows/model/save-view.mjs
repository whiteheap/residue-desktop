// The words /statechart uses for SaveFlow.tla with SaveChart.cfg (one
// folder, and a terminal that can turn the service on): each state as the
// window shows it, each step as what happens.

export const title = "Turning automatic cleanup on, saving folders, turning it off";

const same = (a, b) =>
  a.on === b.on && a.dry === b.dry && a.opts === b.opts && a.odd === b.odd && a.ver === b.ver &&
  a.roots.length === b.roots.length && a.roots.every((r) => b.roots.includes(r));

const read = (v) => ({ on: v.svc.on, roots: v.svc.roots, dry: v.svc.dry, opts: v.svc.opts, odd: v.svc.odd, ver: v.ver });

const saveDialog = (v) => {
  const d = v.dlg;
  if (d.phase === "reading") return "Reading the service";
  if (d.phase === "form") return same(d.shown, read(v)) ? "Shows the service as it is" : "Shows a service that has changed";
  return Object.values(v.calls).some((c) => c.own && c.wrote) ? "Saving: written" : "Saving: nothing written yet";
};

export const state = (v) => {
  if (v.dlg.kind === "on") return ["Save dialog", saveDialog(v)];
  if (v.dlg.kind === "off") return ["Turn-off dialog", v.dlg.phase === "form" ? "Asks to turn it off" : "Turning off"];
  return ["Automatic cleanup", v.svc.on ? "The service is on" : "The service is off"];
};

export const group = (v) => (v.dlg.kind === "none" ? "screen" : "dialog");

export const colors = { screen: "teal", dialog: "blue" };

// The call whose reply arrives: the one that has settled.
const settled = (f) => Object.values(f.calls).find((c) => ["landed", "changed", "failed"].includes(c.phase));

const reply = (f) =>
  settled(f).phase === "landed"
    ? "the dialog closes"
    : settled(f).phase === "changed"
      ? "residue refuses: the service changed"
      : "the call fails";

const labels = {
  ScreenReads: () => "the screen reads the service",
  YouOpenSave: (f) => (f.svc.on ? "you click Change folders" : "you turn it on"),
  DialogReads: () => "the dialog reads the service",
  YouTick: () => "you tick a folder",
  YouPreview: () => "you click Preview",
  PreviewLands: () => "the preview lands",
  PreviewFails: () => "the preview fails",
  YouSave: () => "you save",
  YouOpenTurnOff: () => "you turn it off",
  YouTurnOff: () => "you confirm",
  // Cancel while the dialog reads does what Cancel on the form does.
  YouCancel: (f) => (f.dlg.phase === "reading" ? null : "you cancel"),
  YouLeave: () => "you go to another screen and back",
  InstallChecks: () => "residue reads the service again",
  InstallCopies: () => "the app writes its own copy of residue",
  InstallLands: () => "residue service install writes it",
  InstallFails: () => "the save fails",
  UninstallStarts: () => "the turn-off starts",
  UninstallLands: () => "residue service uninstall removes it",
  UninstallFails: () => "the turn-off fails",
  ReplyArrives: (f) => reply(f),
  Reload: () => "the window starts afresh",
  TerminalTurnsOn: () => "a terminal turns it on",
  TerminalTurnsOff: () => "a terminal turns it off",
  TerminalChangesFolders: () => "a terminal changes its folders",
  TerminalSwitchesMode: () => "a terminal switches its mode",
  TerminalChangesOptions: () => "a terminal changes its options",
  TerminalAddsArgument: () => "a terminal adds an argument",
  BrewUpgrade: () => "brew upgrades residue",
  AppReplaced: () => "a new app is put in place of this one",
};

export const step = (action, from) => (labels[action] ? labels[action](from) : null);
