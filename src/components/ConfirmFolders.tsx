import { Button, Empty } from "@cloudflare/kumo";
import { ArrowClockwise, FolderPlus } from "@phosphor-icons/react";
import { useState } from "react";
import { asCliError } from "../lib/api";
import { tilde } from "../lib/format";
import type { CliError } from "../lib/types";
import { ErrorPanel, SolidButton } from "./ui";

/**
 * The first launch: the usual code folders, scanned only once the user
 * confirms them, since a scan runs Git in every repository under them.
 */
export function ConfirmFolders({
  folders,
  home,
  onConfirm,
  onChooseFolders,
}: {
  folders: string[];
  home: string | undefined;
  onConfirm: (roots: string[]) => Promise<void>;
  onChooseFolders: () => void;
}) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<CliError | null>(null);
  const confirm = async () => {
    setSaving(true);
    setError(null);
    try {
      await onConfirm(folders);
    } catch (e) {
      setError(asCliError(e));
      setSaving(false);
    }
  };
  return (
    <>
      {error ? <ErrorPanel title="The folders were not saved" error={error} /> : null}
      <Empty
        icon={<FolderPlus size={32} />}
        title="Scan these folders?"
        description={`Found in your home folder: ${folders.map((f) => tilde(f, home)).join(", ")}. A scan runs Git in every repository under them, with each repository's own settings.`}
        contents={
          <div className="empty-actions">
            <SolidButton icon={<ArrowClockwise />} loading={saving} disabled={saving} onClick={confirm}>
              Scan these folders
            </SolidButton>
            <Button variant="secondary" icon={<FolderPlus />} disabled={saving} onClick={onChooseFolders}>
              Choose other folders
            </Button>
          </div>
        }
      />
    </>
  );
}
