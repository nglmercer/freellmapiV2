import { $ } from "bun";
import { mkdir, readFile } from "node:fs/promises";

const SUBMODULE_PATH = "getmodelsapi";
const SUBMODULE_NAME = "getmodelsapi";

async function readGitmodulesUrl(): Promise<string | null> {
  try {
    const content = await readFile(".gitmodules", "utf8");
    const block = new RegExp(
      `\\[submodule\\s+"?${SUBMODULE_NAME}"?\\][^\\[]*?url\\s*=\\s*(\\S+)`,
      "s",
    ).exec(content);
    return block?.[1]?.trim() ?? null;
  } catch {
    return null;
  }
}

const exists = await Bun.file(`${SUBMODULE_PATH}/package.json`).exists();

if (exists) {
  console.log(`[submodules] ${SUBMODULE_PATH} already present, skipping init.`);
  process.exit(0);
}

if (!Bun.which("git")) {
  console.error(
    `[submodules] ${SUBMODULE_PATH} is missing and \`git\` is not on PATH.\n` +
      `Initialize the submodule manually:\n` +
      `  git submodule update --init --recursive`,
  );
  process.exit(1);
}

const insideWorkTree = await $`git rev-parse --is-inside-work-tree`.quiet().nothrow();
if (insideWorkTree.exitCode !== 0) {
  console.error(
    `[submodules] Not inside a git working tree; cannot init ${SUBMODULE_PATH}.\n` +
      `Run \`git submodule update --init --recursive\` from a git checkout.`,
  );
  process.exit(1);
}

const url = await readGitmodulesUrl();
if (!url) {
  console.error(
    `[submodules] ${SUBMODULE_PATH} is not declared in .gitmodules.\n` +
      `Add it or run \`git submodule update --init --recursive\` manually.`,
  );
  process.exit(1);
}

console.log(`[submodules] ${SUBMODULE_PATH} missing; attempting \`git submodule update --init\`...`);
const subInit = await $`git submodule update --init --recursive ${SUBMODULE_PATH}`.quiet().nothrow();

if (subInit.exitCode === 0) {
  console.log(`[submodules] ${SUBMODULE_PATH} initialized.`);
  process.exit(0);
}

console.warn(
  `[submodules] \`git submodule update --init\` failed (likely no gitlink in parent index).`,
);
console.log(`[submodules] Falling back to plain clone of ${url} into ${SUBMODULE_PATH}...`);

await mkdir(SUBMODULE_PATH, { recursive: true });

const clone = await $`git clone ${url} ${SUBMODULE_PATH}`.nothrow();
if (clone.exitCode !== 0) {
  console.error(
    `[submodules] Failed to clone ${url} into ${SUBMODULE_PATH}.\n` +
      `Run \`git clone ${url} ${SUBMODULE_PATH}\` manually to see the error.`,
  );
  process.exit(clone.exitCode ?? 1);
}

console.log(`[submodules] ${SUBMODULE_PATH} cloned successfully.`);
