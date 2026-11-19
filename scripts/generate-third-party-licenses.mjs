import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { homedir } from "node:os";

const output = resolve(process.argv[2] ?? "THIRD_PARTY_LICENSES.md");
const noticesOutput = resolve(process.argv[3] ?? "THIRD_PARTY_NOTICES.txt");
const licenseOverrides = new Map([
  ["npm:react-remove-scroll-bar@2.3.8", resolve("third_party/licenses/react-remove-scroll-bar-2.3.8-LICENSE")],
]);
const cargoHome = process.env.CARGO_HOME || join(homedir(), ".cargo");
const cargo = process.env.CARGO
  ?? resolve(cargoHome, "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
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
    ecosystem: "Rust",
    packageRoot: dirname(pkg.manifest_path),
    declaredLicenseFile: pkg.license_file,
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
        ecosystem: "npm",
        packageRoot: packageRootForVersion(pkg, version),
        declaredLicenseFile: null,
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
const notices = collectNotices([...rust, ...frontend]);
writeFileSync(noticesOutput, renderNotices(notices), "utf8");
process.stdout.write(`wrote ${output} and ${noticesOutput} (${rust.length} Rust, ${frontend.length} frontend packages, ${notices.size} unique notice documents)\n`);

function run(program, args) {
  let executable = program;
  let executableArgs = args;
  let windowsVerbatimArguments = false;
  if (process.platform === "win32" && program === "pnpm") {
    const command = findOnPath("pnpm", [".cmd", ".exe", ".bat"]);
    if (!command) throw new Error("pnpm was not found on PATH");
    if (/\.(?:cmd|bat)$/iu.test(command)) {
      const commandLine = [command, ...args].map(quoteCmdArgument).join(" ");
      executable = process.env.ComSpec ?? "cmd.exe";
      executableArgs = ["/d", "/v:off", "/s", "/c", `"${commandLine}"`];
      windowsVerbatimArguments = true;
    } else {
      executable = command;
    }
  }
  const result = spawnSync(executable, executableArgs, {
    cwd: process.cwd(),
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    shell: false,
    windowsVerbatimArguments,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${program} ${args.join(" ")} failed:\n${result.stderr}`);
  }
  return result.stdout;
}

function quoteCmdArgument(value) {
  if (/["%\r\n]/u.test(value)) {
    throw new Error("pnpm command arguments contain unsupported cmd.exe expansion characters");
  }
  return `"${value}"`;
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

function packageRootForVersion(pkg, version) {
  for (const candidate of pkg.paths ?? []) {
    try {
      const manifest = JSON.parse(readFileSync(join(candidate, "package.json"), "utf8"));
      if (manifest.version === version) return candidate;
    } catch {
      // Try the next installed package location.
    }
  }
  return pkg.paths?.[0] ?? null;
}

function collectNotices(packages) {
  const byDigest = new Map();
  const missing = [];
  for (const pkg of packages) {
    const files = licenseFiles(pkg);
    if (files.length === 0) {
      missing.push(`${pkg.ecosystem}:${pkg.name}@${pkg.version}`);
      continue;
    }
    for (const file of files) {
      const content = readFileSync(file.path, "utf8").replaceAll("\r\n", "\n").trim();
      if (!content) continue;
      const digest = createHash("sha256").update(content, "utf8").digest("hex");
      const entry = byDigest.get(digest) ?? { content, packages: [] };
      entry.packages.push({
        ecosystem: pkg.ecosystem,
        name: pkg.name,
        version: pkg.version,
        file: file.name,
      });
      byDigest.set(digest, entry);
    }
  }
  if (missing.length > 0) {
    throw new Error(`packages missing installed license/notice text: ${missing.join(", ")}`);
  }
  return new Map([...byDigest.entries()].sort(([left], [right]) => compare(left, right)));
}

function licenseFiles(pkg) {
  if (!pkg.packageRoot) return [];
  const candidates = new Map();
  const override = licenseOverrides.get(`${pkg.ecosystem}:${pkg.name}@${pkg.version}`);
  if (override) candidates.set(override, { path: override, name: "BA Agent verified upstream override" });
  if (pkg.declaredLicenseFile) {
    const path = resolve(pkg.packageRoot, pkg.declaredLicenseFile);
    try {
      if (statSync(path).isFile()) candidates.set(path, { path, name: pkg.declaredLicenseFile });
    } catch {
      // The package-root scan below may still find conventional license files.
    }
  }
  try {
    for (const entry of readdirSync(pkg.packageRoot, { withFileTypes: true })) {
      if (!entry.isFile() || !/^(?:licen[cs]e|copying|notice|copyright|unlicense)(?:$|[._-])/iu.test(entry.name)) continue;
      const path = join(pkg.packageRoot, entry.name);
      candidates.set(path, { path, name: entry.name });
    }
  } catch {
    return [...candidates.values()];
  }
  return [...candidates.values()].sort((left, right) => compare(left.name.toLowerCase(), right.name.toLowerCase()));
}

function renderNotices(notices) {
  const lines = [
    "BA Agent third-party license and notice texts",
    "",
    "Generated from the installed, locked Rust and embedded-console runtime packages.",
    "Identical documents are deduplicated by SHA-256; every package-to-document mapping remains listed.",
    "",
  ];
  for (const [digest, entry] of notices) {
    lines.push("=".repeat(80), `SHA-256: ${digest}`, "Packages:");
    for (const pkg of entry.packages.sort((left, right) =>
      compare(left.ecosystem, right.ecosystem)
      || compare(left.name, right.name)
      || compare(left.version, right.version)
      || compare(left.file, right.file))) {
      lines.push(`- ${pkg.ecosystem}:${pkg.name}@${pkg.version} (${pkg.file})`);
    }
    lines.push("-".repeat(80), entry.content, "");
  }
  return `${lines.join("\n")}\n`;
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
