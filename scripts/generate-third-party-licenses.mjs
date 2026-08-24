import { spawnSync } from "node:child_process";
import { statSync, writeFileSync } from "node:fs";
import { delimiter, join, resolve } from "node:path";
import { homedir } from "node:os";

const output = resolve(process.argv[2] ?? "THIRD_PARTY_LICENSES.md");
const cargo = process.env.CARGO
  ?? resolve(homedir(), ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
const releaseTargets = [
  "x86_64-pc-windows-msvc",
  "x86_64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
];
const metadataByTarget = releaseTargets.map((target) => JSON.parse(run(cargo, [
  "metadata",
  "--format-version",
  "1",
  "--locked",
  "--filter-platform",
  target,
])));
const packageById = new Map();
const workspace = new Set(metadataByTarget[0].workspace_members);
const reachable = new Set();
for (const metadata of metadataByTarget) {
  for (const pkg of metadata.packages) packageById.set(pkg.id, pkg);
  const root = metadata.packages.find(
    (pkg) => pkg.name === "ba-agent-control" && workspace.has(pkg.id),
  );
  if (!root) throw new Error("unable to locate the ba-agent-control Cargo package");
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const queue = [root.id];
  const targetReachable = new Set();
  while (queue.length > 0) {
    const id = queue.shift();
    if (targetReachable.has(id)) continue;
    targetReachable.add(id);
    reachable.add(id);
    for (const dependency of nodes.get(id)?.deps ?? []) {
      const included = dependency.dep_kinds.length === 0
        || dependency.dep_kinds.some(({ kind }) => kind !== "dev");
      if (included) queue.push(dependency.pkg);
    }
  }
}

const rust = [...packageById.values()]
  .filter((pkg) => reachable.has(pkg.id) && !workspace.has(pkg.id))
  .map((pkg) => ({
    name: pkg.name,
    version: pkg.version,
    license: pkg.license,
    source: pkg.repository ?? pkg.homepage ?? pkg.source ?? "",
  }))
  .sort(byNameAndVersion);
assertKnownLicenses("Rust", rust);

const groupedFrontend = JSON.parse(run("pnpm", ["licenses", "list", "--json", "--prod"]));
const frontendByKey = new Map();
for (const [license, packages] of Object.entries(groupedFrontend)) {
  for (const pkg of packages) {
    for (const version of pkg.versions) {
      const item = {
        name: pkg.name,
        version,
        license: pkg.license ?? license,
        source: pkg.homepage ?? "",
      };
      frontendByKey.set(`${item.name}@${item.version}`, item);
    }
  }
}
const frontend = [...frontendByKey.values()].sort(byNameAndVersion);
assertKnownLicenses("frontend", frontend);

const lines = [
  "# Third-party license inventory",
  "",
  "Generated deterministically from target-filtered `cargo metadata --locked` and `pnpm licenses list --prod` by `pnpm licenses:generate`. Workspace-owned BA Agent packages and development-only Cargo dependencies are excluded.",
  "",
  `Rust target union: ${releaseTargets.map((target) => `\`${target}\``).join(", ")}.`,
  "",
  "This inventory records package license expressions and upstream references. Before Public Beta, release review must verify every expression and include any license or notice text required by the corresponding terms.",
  "",
  `## Rust runtime packages (${rust.length})`,
  "",
  "| Package | Version | License | Upstream |",
  "| --- | --- | --- | --- |",
  ...rust.map(tableRow),
  "",
  `## Embedded console runtime packages (${frontend.length})`,
  "",
  "| Package | Version | License | Upstream |",
  "| --- | --- | --- | --- |",
  ...frontend.map(tableRow),
];
writeFileSync(output, `${lines.join("\n")}\n`, "utf8");
process.stdout.write(`wrote ${output} (${rust.length} Rust, ${frontend.length} frontend packages)\n`);

function run(program, args) {
  let executable = program;
  let executableArgs = args;
  if (process.platform === "win32" && program === "pnpm") {
    const command = findOnPath("pnpm", [".cmd", ".exe", ".bat"]);
    if (!command) throw new Error("pnpm was not found on PATH");
    executable = process.env.ComSpec ?? "cmd.exe";
    executableArgs = ["/d", "/s", "/c", command, ...args];
  }
  const result = spawnSync(executable, executableArgs, {
    cwd: process.cwd(),
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    shell: false,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${program} ${args.join(" ")} failed:\n${result.stderr}`);
  }
  return result.stdout;
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

function assertKnownLicenses(label, packages) {
  const missing = packages.filter((pkg) => !pkg.license).map((pkg) => `${pkg.name}@${pkg.version}`);
  if (missing.length > 0) {
    throw new Error(`${label} packages missing license metadata: ${missing.join(", ")}`);
  }
}

function byNameAndVersion(left, right) {
  return compare(left.name, right.name) || compare(left.version, right.version);
}

function compare(left, right) {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

function tableRow(pkg) {
  return `| ${cell(pkg.name)} | ${cell(pkg.version)} | ${cell(pkg.license)} | ${link(pkg.source)} |`;
}

function cell(value) {
  return String(value).replaceAll("|", "\\|");
}

function link(value) {
  if (!value) return "—";
  if (!/^https?:\/\//u.test(value)) return `\`${cell(value)}\``;
  const safe = String(value).replaceAll("(", "%28").replaceAll(")", "%29");
  return `[source](${safe})`;
}
