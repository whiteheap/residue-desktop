import { Text } from "@cloudflare/kumo";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

export function App() {
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("residue_version").then(setVersion, (e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="grid h-full place-content-center gap-2 text-center">
      <Text variant="heading2" as="h1">
        residue
      </Text>
      <Text variant="secondary">{error ?? version ?? "Starting residue"}</Text>
    </main>
  );
}
