import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { commandView, shellWord } from "../src/lib/format.ts";

/** Reads one word as POSIX shells do: single quotes keep every character, a backslash outside them escapes the next. */
function posixRead(word: string): string {
  let out = "";
  for (let i = 0; i < word.length; i++) {
    const c = word[i];
    if (c === "'") {
      const end = word.indexOf("'", i + 1);
      assert.ok(end > i, `unclosed quote in ${word}`);
      out += word.slice(i + 1, end);
      i = end;
    } else if (c === "\\") {
      out += word[++i] ?? "";
    } else {
      out += c;
    }
  }
  return out;
}

/** Reads one word as fish does: inside single quotes, `\'` and `\\` are escapes and nothing else is. */
function fishRead(word: string): string {
  let out = "";
  for (let i = 0; i < word.length; i++) {
    const c = word[i];
    if (c === "'") {
      for (i++; i < word.length && word[i] !== "'"; i++) {
        if (word[i] === "\\" && (word[i + 1] === "'" || word[i + 1] === "\\")) i++;
        out += word[i];
      }
      assert.ok(i < word.length, `unclosed quote in ${word}`);
    } else if (c === "\\") {
      out += word[++i] ?? "";
    } else {
      out += c;
    }
  }
  return out;
}

// Each one needs quotes. The second is a path that, quoted as before, fish read as a second command.
const QUOTED = [
  "/Users/a/my worktree",
  "/x/a\\';touch PWNED #",
  "it's",
  "back\\slash",
  "\\'",
  "''\\\\",
  "$(echo hi)`x`$HOME",
  "café 😀",
  "=cmd",
  "%self",
  "~/x",
  "*.?[a]{b,c}",
  "",
];

// Each holds a control character, the first as a worktree folder name can.
const CONTROLLED = [
  "/x/y\u001b[201~\u0015touch PWNED2\r",
  "a\u007fb",
  "a\u0085b",
  "a\u2028b",
  "a\u202eb",
  "a\u2066b",
  "a\nb",
  "a\tb",
  "\u001bc9'\\z",
];

test("plain words stay bare", () => {
  for (const w of ["/opt/homebrew/bin/residue", "--grace", "1h", "a_b@c+d,e", "a=b", "50%"]) {
    assert.equal(shellWord(w), w);
  }
});

test("a quoted word reads back alike by POSIX rules and by fish's", () => {
  for (const s of QUOTED) {
    const w = shellWord(s);
    assert.ok(w.startsWith("'"), `${JSON.stringify(s)} gave ${w}`);
    assert.equal(posixRead(w), s, `POSIX reads ${w}`);
    assert.equal(fishRead(w), s, `fish reads ${w}`);
  }
});

test("a word with a control character holds none, and its copy is off", () => {
  assert.equal(shellWord(CONTROLLED[0]!), "$'/x/y\\x1b[201~\\x15touch PWNED2\\x0d'");
  // A hex digit right after an escape is escaped too, for ksh; a quote and a backslash keep their escapes.
  assert.equal(shellWord("\u001bc9'\\z"), "$'\\x1b\\x63\\x39\\'\\\\z'");
  assert.equal(shellWord("\u202e"), "$'\\xe2\\x80\\xae'");
  for (const s of CONTROLLED) {
    const w = shellWord(s);
    assert.ok(w.startsWith("$'"), `${JSON.stringify(s)} gave ${w}`);
    // oxlint-disable-next-line no-control-regex -- finding them is the point.
    assert.doesNotMatch(w, /[\u0000-\u001f\u007f-\u009f\u2028\u2029\u202a-\u202e\u2066-\u2069]/u);
    assert.deepEqual(commandView(`git worktree lock --reason keep ${w}`), {
      text: `git worktree lock --reason keep ${w}`,
      copyable: false,
    });
  }
});

test("a command is copyable only when every shell reads it alike", () => {
  const plain = `git worktree lock --reason keep ${shellWord("/x/my $'wt")}`;
  assert.deepEqual(commandView(plain), { text: plain, copyable: true });
  assert.deepEqual(commandView("/opt/homebrew/bin/residue service uninstall"), {
    text: "/opt/homebrew/bin/residue service uninstall",
    copyable: true,
  });
  // A control character in the text itself is shown escaped, never copied.
  assert.deepEqual(commandView("echo 'a\u001b[201~\u0015b'"), {
    text: "echo 'a\\x1b[201~\\x15b'",
    copyable: false,
  });
  assert.equal(commandView("echo 'unclosed").copyable, false);
});

/** `printf '%s' WORD` in a real shell, or null when that shell is not installed. */
function printed(shell: string[], word: string): { out: string; ok: boolean } | null {
  const [bin, ...args] = shell;
  try {
    const out = execFileSync(bin!, [...args, "-c", `printf '%s' ${word}`], {
      env: { PATH: process.env.PATH ?? "/usr/bin:/bin", LC_ALL: "en_US.UTF-8" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    return { out: out.toString("utf8"), ok: true };
  } catch (e) {
    const err = e as { code?: string; stdout?: Buffer };
    if (err.code === "ENOENT") return null;
    return { out: err.stdout?.toString("utf8") ?? "", ok: false };
  }
}

const SHELLS: Record<string, string[]> = {
  sh: ["sh"],
  bash: ["bash"],
  zsh: ["zsh", "-f"],
  dash: ["dash"],
  ksh: ["ksh"],
  fish: ["fish", "--no-config"],
};

for (const [name, shell] of Object.entries(SHELLS)) {
  test(`${name} reads each quoted word back`, (t) => {
    if (printed(shell, "x") === null) {
      t.skip(`${name} is not installed`);
      return;
    }
    for (const s of QUOTED) {
      assert.deepEqual(printed(shell, shellWord(s)), { out: s, ok: true }, `${name} reads ${shellWord(s)}`);
    }
    for (const s of CONTROLLED) {
      const got = printed(shell, shellWord(s))!;
      if (name === "fish") {
        // fish refuses $'…' and runs nothing.
        assert.deepEqual(got, { out: "", ok: false });
      } else if (name === "bash" || name === "zsh" || name === "ksh") {
        // dash, and an sh that is dash, have no $'…': the window turns copy off for such a word.
        assert.deepEqual(got, { out: s, ok: true }, `${name} reads ${shellWord(s)}`);
      }
    }
  });
}
