import { Button, Checkbox, Dialog } from "@cloudflare/kumo";
import { ArrowClockwise, Eye, FolderPlus, Info } from "@phosphor-icons/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { Command, ErrorPanel, Loading, PageHeader, SolidButton, Toggle } from "../components/ui";
import { api, asCliError } from "../lib/api";
import { shellWord, tilde } from "../lib/format";
import type { AppInfo, CliError, ServiceChange, ServiceStatus } from "../lib/types";
import { useCli } from "../lib/useCli";

export function Automatic({
  info,
  roots,
  suggested,
  home,
  refreshKey,
}: {
  info: AppInfo | null;
  roots: string[];
  suggested: string[];
  home: string | undefined;
  refreshKey: number;
}) {
  const q = useCli(`service:${refreshKey}`, api.serviceStatus);
  const status: ServiceStatus | null = q.data ?? null;
  const error = q.error;
  const loading = q.loading;
  const load = q.reload;
  const [dialog, setDialog] = useState<"on" | "off" | null>(null);
  const [change, setChange] = useState<ServiceChange | null>(null);

  const changed = (c: ServiceChange) => {
    setChange(c);
    setDialog(null);
    load();
  };

  const on = status?.installed ?? false;
  const program = status?.program ?? null;
  const preview = info?.dev_service_preview ?? false;

  return (
    <>
      <PageHeader
        title="Automatic cleanup"
        description="residue watch runs at login and removes a worktree when its work has landed, or when the disk runs low."
        actions={
          <Button variant="secondary" icon={<ArrowClockwise />} loading={loading} disabled={loading} onClick={load}>
            Refresh
          </Button>
        }
      />
      <div className="page-body">
        {preview ? (
          <section className="notice dev">
            <h2>Development preview</h2>
            <p>
              RESIDUE_DESKTOP_DEV_SERVICE=preview is set in this debug build. The switch never runs
              residue service install or uninstall: it shows the command it would run and the
              output of residue watch --once --dry-run, which removes nothing.
            </p>
          </section>
        ) : null}
        {error ? <ErrorPanel title="residue could not read the service" error={error} /> : null}
        {!status && loading ? <Loading title="Reading the service" /> : null}

        {status ? (
          <section className="service">
            <div className="service-switch">
              <Toggle
                checked={on}
                name="Automatic cleanup"
                label={on ? "On" : "Off"}
                busy={loading}
                onChange={(next) => setDialog(next ? "on" : "off")}
              />
              <p className="service-summary">{status.summary}</p>
            </div>
            {on && program ? (
              <dl className="facts">
                <div>
                  <dt>Watching</dt>
                  <dd>
                    {program.roots.length > 0 ? program.roots.map((r) => tilde(r, home)).join(", ") : "no folders"}
                  </dd>
                </div>
                <div>
                  <dt>Runs</dt>
                  <dd>
                    <code>{tilde(program.binary, home)}</code>
                    {program.version ? ` (${program.version})` : ""}
                    {program.managed ? ", the app's own copy, kept outside the app" : ""}
                    {!program.binary_exists ? (
                      <strong className="warn"> missing: turn automatic cleanup off and on again</strong>
                    ) : null}
                  </dd>
                </div>
                {program.dry_run ? (
                  <div>
                    <dt>Mode</dt>
                    <dd>log only (--dry-run): it records what it would remove and removes nothing</dd>
                  </div>
                ) : null}
                {program.options.length > 0 || program.unknown.length > 0 ? (
                  <div>
                    <dt>Options</dt>
                    <dd>
                      <code>{[...program.options, ...program.unknown].join(" ")}</code>
                    </dd>
                  </div>
                ) : null}
                {status.agent_file ? (
                  <div>
                    <dt>Agent file</dt>
                    <dd>
                      <code>{tilde(status.agent_file, home)}</code>
                    </dd>
                  </div>
                ) : null}
              </dl>
            ) : null}
            {on ? (
              <div className="service-actions">
                <Button variant="secondary" onClick={() => setDialog("on")}>
                  Change folders…
                </Button>
              </div>
            ) : null}
          </section>
        ) : null}

        {change ? (
          <section className="block">
            <h2>{change.dev_dry_run ? "Would run" : "Ran"}</h2>
            <Command text={change.command} />
            {change.output.length > 0 ? <pre className="log">{change.output.join("\n")}</pre> : null}
          </section>
        ) : null}

        <WhatItRemoves />

        {status ? (
          <section className="block">
            <h2>Log</h2>
            {status.log_path ? (
              <p className="quiet">
                <code>{tilde(status.log_path, home)}</code>
              </p>
            ) : null}
            {status.log_error ? <p className="hint">{status.log_error}</p> : null}
            {status.log_lines.length > 0 ? (
              <pre className="log full" aria-label="Recent log lines">
                {status.log_lines.join("\n")}
              </pre>
            ) : (
              <p className="quiet">No log lines yet.</p>
            )}
          </section>
        ) : null}

        <section className="block">
          <h2>Without the app</h2>
          <p className="quiet">The service does not need the app. To turn it off from a terminal:</p>
          {/* The binary the service runs: the app's own copy is not on PATH. */}
          <Command
            text={`${program ? shellWord(program.binary) : "residue"} service uninstall`}
          />
          {info?.path_source === "fallback" ? (
            <p className="hint">
              The app could not read your login shell's PATH, so residue runs with the standard
              folders only. If Git lives somewhere else, the service may not find it.
            </p>
          ) : null}
        </section>
      </div>

      {dialog === "on" ? (
        <TurnOnDialog
          initial={program?.roots.length ? program.roots : roots.length ? roots : suggested}
          suggested={suggested}
          home={home}
          replacing={on}
          initialDryRun={program?.dry_run ?? false}
          onClose={() => setDialog(null)}
          onChanged={changed}
        />
      ) : null}
      {dialog === "off" ? <TurnOffDialog onClose={() => setDialog(null)} onChanged={changed} /> : null}
    </>
  );
}

function WhatItRemoves() {
  return (
    <section className="block what">
      <h2>
        <Info aria-hidden /> What it removes
      </h2>
      <ul className="plain-list bullets">
        <li>
          A worktree whose work landed: its upstream branch was deleted, as merging a pull request
          with branch deletion does, and the worktree has not changed for an hour. residue never
          fetches; it notices after your next git fetch --prune.
        </li>
        <li>
          When a disk is below 10% free: worktrees unchanged for two days, largest first, until
          15% is free.
        </li>
      </ul>
      <p>
        Every check runs before each removal, so a worktree with changes, untracked files or a
        running process stays. Automatic cleanup deletes the worktree instead of using the trash,
        because freeing the disk is its job. The branch and its commits stay, and a locked worktree
        stays. It also deletes trash entries older than 7 days.
      </p>
    </section>
  );
}

function TurnOnDialog({
  initial,
  suggested,
  home,
  replacing,
  initialDryRun,
  onClose,
  onChanged,
}: {
  initial: string[];
  suggested: string[];
  home: string | undefined;
  replacing: boolean;
  initialDryRun: boolean;
  onClose: () => void;
  onChanged: (c: ServiceChange) => void;
}) {
  const [options, setOptions] = useState<string[]>(() => [...new Set([...initial, ...suggested])]);
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(initial));
  const [dryRun, setDryRun] = useState(initialDryRun);
  const [working, setWorking] = useState<"preview" | "install" | null>(null);
  const [previewLines, setPreviewLines] = useState<string[] | null>(null);
  const [error, setError] = useState<CliError | null>(null);

  const roots = options.filter((o) => chosen.has(o));

  const add = async () => {
    const picked = await openDialog({ directory: true, multiple: true, title: "Folders to watch" });
    const list = picked == null ? [] : Array.isArray(picked) ? picked : [picked];
    if (list.length === 0) return;
    setOptions((o) => [...new Set([...o, ...list])]);
    setChosen((c) => new Set([...c, ...list]));
    setPreviewLines(null);
  };

  const toggle = (root: string, on: boolean) => {
    setChosen((c) => {
      const next = new Set(c);
      if (on) next.add(root);
      else next.delete(root);
      return next;
    });
    setPreviewLines(null);
  };

  const preview = async () => {
    setWorking("preview");
    setError(null);
    try {
      setPreviewLines(await api.servicePreview(roots));
    } catch (e) {
      setError(asCliError(e));
    } finally {
      setWorking(null);
    }
  };

  const install = async () => {
    setWorking("install");
    setError(null);
    try {
      onChanged(await api.serviceInstall(roots, dryRun));
    } catch (e) {
      setError(asCliError(e));
      setWorking(null);
    }
  };

  return (
    <Dialog.Root
      role="alertdialog"
      open
      onOpenChange={(open) => {
        if (!open && !working) onClose();
      }}
    >
      <Dialog size="xl" className="dialog">
        <Dialog.Title className="dialog-title">
          {replacing ? "Change the watched folders" : "Turn on automatic cleanup"}
        </Dialog.Title>
        <Dialog.Description className="dialog-text">
          residue watch starts now and at every login. It deletes a worktree in the folders below
          when its upstream branch is gone and it has not changed for an hour, or when the disk is
          below 10% free and it has not changed for two days. Every check runs first. It deletes
          instead of using the trash; branches and commits stay.
        </Dialog.Description>

        <fieldset className="folder-choice">
          <legend>Folders to watch</legend>
          {options.map((root) => (
            <Checkbox
              key={root}
              label={tilde(root, home)}
              checked={chosen.has(root)}
              onCheckedChange={(on) => toggle(root, on === true)}
            />
          ))}
          <Button variant="ghost" size="sm" icon={<FolderPlus />} onClick={add}>
            Add a folder…
          </Button>
        </fieldset>

        <Checkbox
          label="Log only: record what it would remove, and remove nothing (--dry-run)"
          checked={dryRun}
          onCheckedChange={(on) => setDryRun(on === true)}
        />

        <section className="plan-section">
          <div className="plan-section-head">
            <h3>What it would remove right now</h3>
            <Button
              variant="secondary"
              size="sm"
              icon={<Eye />}
              loading={working === "preview"}
              disabled={working !== null || roots.length === 0}
              onClick={preview}
            >
              Preview
            </Button>
          </div>
          {previewLines ? (
            <pre className="log full" aria-label="Preview">
              {previewLines.length > 0 ? previewLines.join("\n") : "Nothing to remove right now."}
            </pre>
          ) : (
            <p className="hint">Runs residue watch --once --dry-run on these folders. It removes nothing.</p>
          )}
        </section>

        {error ? <ErrorPanel title="residue did not finish" error={error} /> : null}

        <div className="dialog-buttons">
          <Button variant="secondary" onClick={onClose} disabled={working !== null}>
            Cancel
          </Button>
          <SolidButton
            loading={working === "install"}
            disabled={working !== null || roots.length === 0}
            onClick={install}
          >
            {replacing ? "Save" : "Turn on"}
          </SolidButton>
        </div>
      </Dialog>
    </Dialog.Root>
  );
}

function TurnOffDialog({ onClose, onChanged }: { onClose: () => void; onChanged: (c: ServiceChange) => void }) {
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<CliError | null>(null);
  const off = async () => {
    setWorking(true);
    setError(null);
    try {
      onChanged(await api.serviceUninstall());
    } catch (e) {
      setError(asCliError(e));
      setWorking(false);
    }
  };
  return (
    <Dialog.Root
      role="alertdialog"
      open
      onOpenChange={(open) => {
        if (!open && !working) onClose();
      }}
    >
      <Dialog size="lg" className="dialog">
        <Dialog.Title className="dialog-title">Turn off automatic cleanup?</Dialog.Title>
        <Dialog.Description className="dialog-text">
          residue service uninstall stops residue watch and removes its login item. Worktrees stay
          until you remove them. The log stays.
        </Dialog.Description>
        {error ? <ErrorPanel title="residue could not turn it off" error={error} /> : null}
        <div className="dialog-buttons">
          <Button variant="secondary" onClick={onClose} disabled={working}>
            Cancel
          </Button>
          <SolidButton loading={working} disabled={working} onClick={off}>
            Turn off
          </SolidButton>
        </div>
      </Dialog>
    </Dialog.Root>
  );
}
