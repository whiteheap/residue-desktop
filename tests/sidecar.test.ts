// scripts/sidecar.sh places the residue the app bundles. Only it fills
// src-tauri/binaries: every mode refuses a file there that git tracks, and
// the modes CI and the release run, --require-attestation and --check, never
// keep a placement on the word of .placed alone. Each case runs a copy of the
// script in a scratch repository, with gh and curl replaced by stand-ins that
// record each call and fail, so nothing is downloaded.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, describe, test } from "node:test";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..");
const scratch = mkdtempSync(join(tmpdir(), "residue-sidecar-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const BINARIES = ["aarch64-apple-darwin", "x86_64-apple-darwin", "universal-apple-darwin"].map(
  (t) => `residue-cli-${t}`,
);

function lockValue(key: string): string {
  const line = readFileSync(join(repo, "sidecar.lock"), "utf8")
    .split("\n")
    .find((l) => l.startsWith(`${key}=`));
  assert.ok(line, `sidecar.lock has ${key}`);
  return line.slice(key.length + 1);
}

/** .placed's first line for the pinned release, as the script writes it. */
const releaseLine = () =>
  `release ${lockValue("version")} ${lockValue("sha256_aarch64-apple-darwin")} ${lockValue("sha256_x86_64-apple-darwin")}`;

let count = 0;

/** A scratch copy of the repository's sidecar files; a git repository unless `git` is false. */
function fixture({ git = true } = {}) {
  const dir = join(scratch, `case-${++count}`);
  mkdirSync(join(dir, "scripts"), { recursive: true });
  mkdirSync(join(dir, "src-tauri/binaries"), { recursive: true });
  mkdirSync(join(dir, "shims"));
  for (const f of ["scripts/sidecar.sh", "sidecar.lock", ".gitignore"]) {
    copyFileSync(join(repo, f), join(dir, f));
  }
  const log = join(dir, "calls.log");
  writeFileSync(log, "");
  for (const tool of ["gh", "curl"]) {
    const shim = join(dir, "shims", tool);
    writeFileSync(shim, `#!/bin/sh\necho "${tool} $*" >> "${log}"\nexit 1\n`);
    chmodSync(shim, 0o755);
  }
  const env = {
    PATH: `${join(dir, "shims")}:/usr/bin:/bin`,
    HOME: dir,
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
  };
  const run = (...args: string[]) => {
    const r = spawnSync("/bin/sh", ["scripts/sidecar.sh", ...args], { cwd: dir, env, encoding: "utf8" });
    return { status: r.status, stderr: r.stderr, stdout: r.stdout };
  };
  const gitRun = (...args: string[]) => {
    const r = spawnSync("git", ["-c", "user.name=test", "-c", "user.email=test@example.invalid", ...args], {
      cwd: dir,
      env,
      encoding: "utf8",
    });
    assert.equal(r.status, 0, r.stderr);
  };
  if (git) {
    gitRun("init", "-q");
    gitRun("add", "scripts/sidecar.sh", "sidecar.lock", ".gitignore");
    gitRun("commit", "-q", "-m", "fixture");
  }
  return {
    dir,
    run,
    git: gitRun,
    calls: () => readFileSync(log, "utf8").trim(),
    /**
     * Binaries that are not residue, and a .placed that says they are the
     * attested release, with their sha256 on a second line as the script
     * once recorded them.
     */
    forge() {
      const sums = BINARIES.map((name) => {
        const file = join(dir, "src-tauri/binaries", name);
        const body = `#!/bin/sh\necho residue ${lockValue("version")} # ${name}\n`;
        writeFileSync(file, body);
        chmodSync(file, 0o755);
        return createHash("sha256").update(body).digest("hex");
      });
      writeFileSync(join(dir, "src-tauri/binaries/.placed"), `${releaseLine()} attested\nbinaries ${sums.join(" ")}\n`);
    },
    /** The placement this repository's own run of the script made. */
    copyPlaced() {
      for (const name of [...BINARIES, ".placed"]) {
        copyFileSync(join(repo, "src-tauri/binaries", name), join(dir, "src-tauri/binaries", name));
      }
    },
  };
}

const MODES: string[][] = [[], ["--require-attestation"], ["--check"]];
const label = (mode: string[]) => mode[0] ?? "(place)";

describe("sidecar.sh", { skip: process.platform !== "darwin" && "the script runs on macOS only" }, () => {
  for (const mode of MODES) {
    test(`${label(mode)} refuses files a commit put in src-tauri/binaries`, () => {
      const f = fixture();
      f.forge();
      f.git("add", "-f", "src-tauri/binaries");
      f.git("commit", "-q", "-m", "forced");
      const r = f.run(...mode);
      assert.notEqual(r.status, 0);
      assert.match(r.stderr, /git tracks files in src-tauri\/binaries/);
      assert.equal(f.calls(), "", "nothing was downloaded or checked");
    });
  }

  test("--check refuses binaries that are not the ones sidecar.lock pins, whatever .placed says", () => {
    const f = fixture();
    f.forge();
    const r = f.run("--check");
    assert.notEqual(r.status, 0);
    assert.match(r.stderr, /does not hold residue-cli .* as sidecar\.lock pins it/);
  });

  test("--require-attestation checks again whatever is placed", () => {
    const f = fixture();
    f.forge();
    const r = f.run("--require-attestation");
    assert.notEqual(r.status, 0);
    assert.match(r.stderr, /gh is not installed or not logged in/);
  });

  test("a plain run does not keep binaries that are not the pinned ones", () => {
    const f = fixture();
    f.forge();
    const r = f.run();
    assert.notEqual(r.status, 0);
    assert.match(r.stderr, /could not download/);
    assert.match(f.calls(), /^curl /m);
  });

  test("--check fails where git cannot list the files", () => {
    const f = fixture({ git: false });
    const r = f.run("--check");
    assert.notEqual(r.status, 0);
    assert.match(r.stderr, /git cannot list the files/);
  });

  // Control: the real placement, which this repository's own run of the
  // script made and checked, passes, and the same files committed do not.
  const real = spawnSync("/bin/sh", ["scripts/sidecar.sh", "--check"], { cwd: repo, encoding: "utf8" });
  const placed = BINARIES.every((n) => existsSync(join(repo, "src-tauri/binaries", n))) && real.status === 0;
  test(
    "the pinned release, placed, passes --check and is kept, with nothing downloaded",
    { skip: !placed && "run sh scripts/sidecar.sh --require-attestation first" },
    () => {
      const f = fixture();
      f.copyPlaced();
      assert.equal(f.run("--check").status, 0);
      const r = f.run();
      assert.equal(r.status, 0, r.stderr);
      assert.match(r.stdout, /is in src-tauri\/binaries/);
      assert.equal(f.calls(), "");
      f.git("add", "-f", "src-tauri/binaries");
      f.git("commit", "-q", "-m", "forced");
      assert.notEqual(f.run("--check").status, 0);
    },
  );
});
