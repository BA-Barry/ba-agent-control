import { readFileSync, statSync } from "node:fs";
import { delimiter, join } from "node:path";
import { homedir } from "node:os";
import { spawnSync } from "node:child_process";

const gate = process.argv[2];
const cargoHome = process.env.CARGO_HOME || join(homedir(), ".cargo");
const cargo = resolveExecutable(
  process.env.CARGO,
  join(cargoHome, "bin", process.platform === "win32" ? "cargo.exe" : "cargo"),
  "cargo",
);
const cargoOptions = preferredCargoOptions();
const commands = {
  format: [cargo, ["fmt", "--all", "--", "--check"], cargoOptions],
  lint: [cargo, ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], cargoOptions],
  typecheck: ["pnpm", ["typecheck"]],
};

if (gate === "secret-scan") {
  secretScan();
} else if (gate === "diff-review") {
  run("git", ["diff", "--check"]);
  run("git", ["diff", "--cached", "--check"]);
} else if (gate === "changed-tests") {
  run("pnpm", ["--filter", "@ba-agent/console", "build"]);
  run(cargo, ["test", "--workspace"], cargoOptions);
} else if (Object.hasOwn(commands, gate)) {
  const [program, args, options] = commands[gate];
  run(program, args, options);
} else {
  process.stderr.write(`unknown quality gate: ${gate ?? "<missing>"}\n`);
  process.exitCode = 2;
}

function resolveExecutable(...candidates) {
  return candidates.find((candidate) => candidate && executableExists(candidate)) ?? candidates.at(-1);
}

function executableExists(program) {
  if (program.includes("/") || program.includes("\\")) {
    try {
      return statSync(program).isFile();
    } catch {
      return false;
    }
  }
  const extensions = process.platform === "win32" ? [".exe", ".cmd", ".bat", ""] : [""];
  return (process.env.PATH ?? "")
    .split(delimiter)
    .some((directory) => extensions.some((extension) => {
      try {
        return statSync(join(directory, `${program}${extension}`)).isFile();
      } catch {
        return false;
      }
    }));
}

function preferredCargoOptions() {
  if (process.platform !== "win32") return {};
  const rustup = resolveExecutable(
    process.env.RUSTUP,
    join(cargoHome, "bin", "rustup.exe"),
    "rustup",
  );
  const list = spawnSync(rustup, ["toolchain", "list"], {
    encoding: "utf8",
    shell: false,
  });
  const gnuStable = list.status === 0
    ? list.stdout.split(/\r?\n/).map((line) => line.split(" ")[0]).find((name) => /^stable-.*-pc-windows-gnu$/.test(name))
    : null;
  return gnuStable
    ? { env: { ...process.env, RUSTUP_TOOLCHAIN: gnuStable } }
    : {};
}

function run(program, args, options = {}) {
  let executable = program;
  let executableArgs = args;
  if (process.platform === "win32" && !program.includes("\\") && !program.endsWith(".exe")) {
    const resolved = findOnPath(program, [".cmd", ".exe", ".bat", ""]);
    if (resolved?.endsWith(".cmd") || resolved?.endsWith(".bat")) {
      executable = process.env.ComSpec ?? "cmd.exe";
      executableArgs = ["/d", "/s", "/c", resolved, ...args];
    } else if (resolved) {
      executable = resolved;
    }
  }
  const result = spawnSync(executable, executableArgs, {
    cwd: process.cwd(),
    stdio: "inherit",
    shell: false,
    ...options,
  });
  if (result.error) {
    process.stderr.write(`${result.error.message}\n`);
    process.exit(result.status ?? 1);
  }
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function findOnPath(program, extensions) {
  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    for (const extension of extensions) {
      const candidate = join(directory, `${program}${extension}`);
      try {
        if (statSync(candidate).isFile()) return candidate;
      } catch {
        // Continue to the next explicit candidate.
      }
    }
  }
  return null;
}

function secretScan() {
  const listing = spawnSync("git", [
    "ls-files",
    "--cached",
    "--others",
    "--exclude-standard",
    "-z",
    "--",
    ":!**/node_modules/**",
    ":!target/**",
  ], {
    cwd: process.cwd(),
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
    shell: false,
  });
  if (listing.status !== 0) {
    process.stderr.write(listing.stderr || "unable to enumerate repository files\n");
    process.exit(listing.status ?? 1);
  }
  const rules = [
    ["OpenAI-style API key", new RegExp(`sk-${"[A-Za-z0-9]"}{32,}`)],
    ["Anthropic-style API key", new RegExp(`sk-${"ant-"}[A-Za-z0-9_-]{24,}`)],
    ["GitHub token", new RegExp(`gh[opusr]_${"[A-Za-z0-9]"}{30,}`)],
    ["PEM private key", new RegExp(`BEGIN ${"(?:RSA |EC |OPENSSH )?"}PRIVATE KEY`)],
  ];
  const findings = [];
  for (const path of listing.stdout.split("\0").filter(Boolean)) {
    let buffer;
    try {
      if (statSync(path).size > 1024 * 1024) continue;
      buffer = readFileSync(path);
    } catch {
      continue;
    }
    if (buffer.includes(0)) continue;
    const text = buffer.toString("utf8");
    for (const [name, pattern] of rules) {
      if (pattern.test(text)) findings.push(`${path}: ${name}`);
    }
  }
  if (findings.length > 0) {
    process.stderr.write(`secret scan failed:\n${findings.join("\n")}\n`);
    process.exit(1);
  }
  process.stdout.write("secret scan passed\n");
}
