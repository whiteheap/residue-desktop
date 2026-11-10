import { Button, Dialog } from "@cloudflare/kumo";
import { Trash } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { ErrorPanel, Flags, Loading, SolidButton } from "../components/ui";
import { api, asCliError } from "../lib/api";
import { ageOf, baseName, bytes, plural, span, tilde } from "../lib/format";
import { removalLine } from "../lib/freed";
import { planLists, shownOf } from "../lib/plan";
import { removedBy } from "../lib/records";
import type { CliError, RemoveDocument } from "../lib/types";
import { documentTime } from "../lib/useCli";
import { displayPath, keptBy, checkLabel, summary } from "../lib/worktree";

type Folders = Record<string, string>;

type Phase =
  | { step: "planning" }
  | { step: "plan"; plan: RemoveDocument; folders: Folders }
  | { step: "running"; plan: RemoveDocument; folders: Folders }
  | { step: "done"; result: RemoveDocument; dropped: string[]; unlisted: string[] }
  | { step: "error"; error: CliError; plan: RemoveDocument | null };

/**
 * Move worktrees to the trash. First residue's own plan (`residue remove
 * --trash --json PATH...` without --yes, which removes nothing): each worktree
 * checked again, every expired trash entry the run may purge, and every
 * removal record it may forget. The user confirms that list, and the confirm
 * sends it back (`shownOf`). src-tauri plans again at the click, with the
 * confirmed command without --yes, leaves out a worktree that changed (the
 * result names it), and runs nothing when the plan covers an entry or record
 * the dialog did not list; the dialog then plans again and says what
 * changed. Otherwise it runs the command with --yes and --as-of the shown
 * plan's time.
 */
export function RemoveDialog({
  paths,
  home,
  estimateNote,
  onClose,
  onRemoved,
}: {
  paths: string[];
  home: string | undefined;
  /** residue's sentence on how it measures sizes, from its stats, shown with the freed line. */
  estimateNote: string | undefined;
  onClose: () => void;
  onRemoved: (doc: RemoveDocument) => void;
}) {
  const [phase, setPhase] = useState<Phase>({ step: "planning" });
  // What changed since the plan before this one, when a confirm found it changed.
  const [changed, setChanged] = useState<string[] | null>(null);

  useEffect(() => {
    let live = true;
    api
      .planRemoval(paths)
      .then((r) => live && setPhase({ step: "plan", plan: r.document, folders: r.folders }))
      .catch((e) => live && setPhase({ step: "error", error: asCliError(e), plan: null }));
    return () => {
      live = false;
    };
  }, [paths]);

  const busy = phase.step === "planning" || phase.step === "running";

  const confirm = (plan: RemoveDocument, folders: Folders) => {
    setPhase({ step: "running", plan, folders });
    setChanged(null);
    api
      .removeToTrash(shownOf(plan, folders))
      .then((r) => {
        if (r.outcome === "changed") {
          // Nothing ran. Plan the same choice again, and say what changed.
          // The dialog stays open while it plans (`busy`).
          setChanged(r.changes);
          setPhase({ step: "planning" });
          api
            .planRemoval(paths)
            .then((p) => setPhase({ step: "plan", plan: p.document, folders: p.folders }))
            .catch((e) => setPhase({ step: "error", error: asCliError(e), plan: null }));
          return;
        }
        setPhase({ step: "done", result: r.document, dropped: r.dropped, unlisted: r.unlisted });
        onRemoved(r.document);
      })
      .catch((e) => setPhase({ step: "error", error: asCliError(e), plan }));
  };

  return (
    <Dialog.Root
      role="alertdialog"
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <Dialog size="xl" className="dialog">
        <Dialog.Title className="dialog-title">
          {phase.step === "done" ? "Done" : "Move to the trash"}
        </Dialog.Title>
        {phase.step === "planning" ? (
          <>
            <Dialog.Description className="dialog-text">
              Checking {plural(paths.length, "worktree")} again. Nothing moves yet.
            </Dialog.Description>
            <Loading title="Checking again" />
          </>
        ) : null}

        {phase.step === "plan" || phase.step === "running" ? (
          <Plan
            plan={phase.plan}
            home={home}
            changed={changed}
            running={phase.step === "running"}
            onCancel={onClose}
            onConfirm={() => confirm(phase.plan, phase.folders)}
          />
        ) : null}

        {phase.step === "done" ? (
          <Result
            doc={phase.result}
            dropped={phase.dropped}
            unlisted={phase.unlisted}
            home={home}
            estimateNote={estimateNote}
            onClose={onClose}
          />
        ) : null}

        {phase.step === "error" ? (
          <>
            <Dialog.Description className="dialog-text">
              {/* Kind invalid: the app ran nothing, such as while another removal runs (#12). */}
              {!phase.plan
                ? "residue could not make a plan, so nothing moved."
                : phase.error.kind === "invalid"
                  ? "This removal did not start, so nothing moved."
                  : "residue stopped before it reported. Scan again to see what moved."}
            </Dialog.Description>
            <ErrorPanel
              title={phase.error.kind === "invalid" ? "residue remove did not start" : "residue remove failed"}
              error={phase.error}
            />
            <div className="dialog-buttons">
              <SolidButton onClick={onClose}>Close</SolidButton>
            </div>
          </>
        ) : null}
      </Dialog>
    </Dialog.Root>
  );
}

function Plan({
  plan,
  home,
  changed,
  running,
  onCancel,
  onConfirm,
}: {
  plan: RemoveDocument;
  home: string | undefined;
  /** What changed since the plan a confirm was refused for, or null. */
  changed: string[] | null;
  running: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { ready, kept, purges, mayPurge, forgets, mayForget } = planLists(plan);
  const total = ready.reduce((sum, r) => sum + (r.worktree?.size?.reclaimable_bytes ?? 0), 0);
  const now = documentTime(plan.generated_at);
  const recordsKept = span(plan.removal_records.keep_seconds);

  return (
    <>
      <Dialog.Description className="dialog-text">
        {ready.length > 0
          ? `Branches and commits stay. Restore brings a worktree back whole for at least ${span(plan.trash.keep_seconds)}, then from its removal record, without ignored files, for at least ${recordsKept}.`
          : "residue keeps all of these, so nothing moves."}
      </Dialog.Description>

      {changed ? (
        <div className="notice" role="alert">
          <p>
            <strong className="warn">Nothing moved: the plan changed after it was shown. </strong>
            Check it as it is now, then confirm again.
          </p>
          <ul>
            {changed.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </div>
      ) : null}

      {ready.length > 0 ? (
        <section className="plan-section">
          <h3>
            To the trash <span className="num">{bytes(total)}</span>
          </h3>
          <ul className="plan-list">
            {ready.map((r) => {
              const p = r.worktree ? displayPath(r.worktree) : r.path;
              return (
                <li key={r.path}>
                  <span className="plan-name">{baseName(p)}</span>
                  <span className="plan-path">{tilde(p, home)}</span>
                  <span className="num">{bytes(r.worktree?.size?.reclaimable_bytes)}</span>
                </li>
              );
            })}
          </ul>
          <p className="hint">
            Frees no space yet: the first removal or automatic cleanup in its repository after{" "}
            {span(plan.trash.keep_seconds)} deletes it and frees the {bytes(total)}.
          </p>
        </section>
      ) : null}

      {kept.length > 0 ? (
        <section className="plan-section">
          <h3>Stays</h3>
          <ul className="plan-list">
            {kept.map((r) => {
              const p = r.worktree ? displayPath(r.worktree) : r.path;
              const why = r.worktree ? keptBy(r.worktree)[0] : undefined;
              return (
                <li key={r.path}>
                  <span className="plan-name">{baseName(p)}</span>
                  <span className="plan-path">
                    {why ? `${checkLabel(why.id)}: ${why.detail}` : (r.reason ?? r.result)}
                  </span>
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      {purges.length > 0 ? (
        <section className="plan-section">
          <h3>Deleted from the trash</h3>
          <p className="hint">
            Older than {span(plan.trash.keep_seconds)}. Each leaves a removal record: Restore recreates
            the worktree from it, without ignored files, for at least {recordsKept}.
          </p>
          <ul className="plan-list">
            {purges.map((p) => (
              <li key={p.entry.entry_path}>
                <span className="plan-name">{p.entry.entry}</span>
                <span className="plan-path">{tilde(p.entry.original_path ?? p.entry.entry_path, home)}</span>
                <span className="num">{bytes(p.entry.reclaimable_bytes)}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {mayPurge.length > 0 ? (
        <section className="plan-section">
          <h3>May be deleted from the trash</h3>
          <p className="hint">
            Older than {span(plan.trash.keep_seconds)}, but residue keeps each for now. The run checks each
            again and deletes it if every check passes then, leaving a removal record.
          </p>
          <ul className="plan-list">
            {mayPurge.map((p) => (
              <li key={p.entry.entry_path}>
                <span className="plan-name">{p.entry.entry}</span>
                <span className="plan-path">
                  {p.worktree && keptBy(p.worktree).length > 0 ? summary(p.worktree, now ?? 0) : (p.reason ?? p.result)}
                </span>
                <span className="num">{bytes(p.entry.reclaimable_bytes)}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {forgets.length > 0 ? (
        <section className="plan-section">
          <h3>Removal records forgotten</h3>
          <p className="hint">
            Each is older than both {recordsKept} (<Flags flags={["--keep-removed"]} />) and the{" "}
            <Flags flags={["--keep-removed"]} /> it was written with. Restore can no longer bring these
            back. Branches stay; a commit only the record held, such as one on a deleted branch, is
            left to the next git gc, which can delete it.
          </p>
          <ul className="plan-list">
            {forgets.map((f) => (
              <li key={f.entry.entry_path}>
                <span className="plan-name">{baseName(f.entry.original_path ?? f.entry.entry)}</span>
                <span className="plan-path">
                  {/* An unfinished record residue cannot read is forgotten an hour on; its error says why. */}
                  {f.entry.error ??
                    ["removed", ageOf(f.entry.removed_at, now), removedBy(f.entry.reason)].filter(Boolean).join(" ")}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {mayForget.length > 0 ? (
        <section className="plan-section">
          <h3>May be forgotten</h3>
          <p className="hint">
            Older than {recordsKept}, but residue keeps each for now. The run looks at each again and forgets
            it if it can then.
          </p>
          <ul className="plan-list">
            {mayForget.map((f) => (
              <li key={f.entry.entry_path}>
                <span className="plan-name">{baseName(f.entry.original_path ?? f.entry.entry)}</span>
                <span className="plan-path">{f.reason ?? f.result}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {plan.errors.length > 0 ? (
        <ul className="plan-errors">
          {plan.errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      ) : null}

      {plan.warnings.length > 0 ? (
        <section className="plan-section">
          <h3>Warnings</h3>
          <ul className="plan-errors">
            {plan.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </section>
      ) : null}

      <div className="dialog-buttons">
        <Button variant="secondary" onClick={onCancel} disabled={running}>
          Cancel
        </Button>
        {ready.length > 0 ? (
          <SolidButton
            icon={<Trash />}
            loading={running}
            disabled={running}
            onClick={onConfirm}
          >
            {running ? "Moving…" : `Move ${plural(ready.length, "worktree")} to the trash`}
          </SolidButton>
        ) : null}
      </div>
    </>
  );
}

const RESULT_TEXT: Record<string, string> = {
  trashed: "in the trash",
  removed: "deleted",
  refused: "kept",
  skipped: "not moved",
  failed: "failed",
  invalid: "not a worktree",
};

function Result({
  doc,
  dropped,
  unlisted,
  home,
  estimateNote,
  onClose,
}: {
  doc: RemoveDocument;
  /** Each worktree the plan showed that the run left out because it changed, one line each. */
  dropped: string[];
  /** What the run moved, purged or forgot that the plan did not show, one line each. */
  unlisted: string[];
  home: string | undefined;
  estimateNote: string | undefined;
  onClose: () => void;
}) {
  const freed = removalLine(doc.stats, doc.trash.keep);
  const moved = doc.results.filter((r) => r.result === "trashed");
  const purged = doc.purged.filter((p) => p.result === "purged");
  const forgotten = doc.forgotten.filter((f) => f.result === "forgotten");
  const other = doc.results.filter((r) => r.result !== "trashed");
  return (
    <>
      <Dialog.Description className="dialog-text">
        {moved.length > 0
          ? `${plural(moved.length, "worktree")} moved to the trash. Restore puts each one back.`
          : "Nothing moved."}
      </Dialog.Description>
      <ul className="plan-list">
        {doc.results.map((r) => {
          const p = r.worktree ? displayPath(r.worktree) : r.path;
          return (
            <li key={r.path} data-result={r.result}>
              <span className="plan-name">{baseName(p)}</span>
              <span className="plan-path" title={r.reason ?? undefined}>
                {r.reason && r.result !== "trashed" ? r.reason : tilde(p, home)}
              </span>
              <span className="plan-result">{RESULT_TEXT[r.result] ?? r.result}</span>
            </li>
          );
        })}
      </ul>
      {freed ? (
        // residue's own line, as `residue remove` ends, with its note on how sizes are measured.
        <p className="hint">
          {freed.charAt(0).toUpperCase()}
          {freed.slice(1)}.{estimateNote ? ` ${estimateNote}` : ""}
        </p>
      ) : null}
      {purged.length > 0 ? (
        <p className="hint">
          Deleted {plural(purged.length, "old trash entry", "old trash entries")}, freeing{" "}
          {bytes(purged.reduce((s, p) => s + (p.entry.reclaimable_bytes ?? 0), 0))}. Each left a
          removal record in Restore.
        </p>
      ) : null}
      {forgotten.length > 0 ? (
        <p className="hint">
          Forgot {plural(forgotten.length, "removal record")}, each older than both{" "}
          {span(doc.removal_records.keep_seconds)} and the <Flags flags={["--keep-removed"]} /> it
          was written with.
        </p>
      ) : null}
      {other.length > 0 || doc.errors.length > 0 ? (
        <ul className="plan-errors">
          {doc.errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      ) : null}
      {dropped.length > 0 ? (
        <section className="plan-section" role="alert">
          <h3>Left out: changed since the plan</h3>
          <p className="hint">These stayed where they are. Scan again to see them as they are now.</p>
          <ul className="plan-errors">
            {dropped.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </section>
      ) : null}
      {unlisted.length > 0 ? (
        <section className="plan-section" role="alert">
          <h3>Not as the plan showed</h3>
          <p className="hint">
            These changed in the moment between the app's last check and residue's run. Restore puts each
            one back.
          </p>
          <ul className="plan-errors">
            {unlisted.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </section>
      ) : null}
      {doc.warnings.length > 0 ? (
        <section className="plan-section">
          <h3>Warnings</h3>
          <ul className="plan-errors">
            {doc.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </section>
      ) : null}
      <div className="dialog-buttons">
        <SolidButton onClick={onClose}>
          Close
        </SolidButton>
      </div>
    </>
  );
}
