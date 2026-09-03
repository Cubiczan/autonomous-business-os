#!/usr/bin/env node
/**
 * @cubiczan/autonomous-business-os-mcp
 *
 * Launches the Autonomous Business OS stdio MCP (`python -m app.mcp`).
 * CHP is the lock; this package is the pipe. Spend gates stay in @cubiczan/chp-mcp.
 */
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

function findRepoRoot() {
  if (process.env.ABOS_REPO_ROOT) {
    return path.resolve(process.env.ABOS_REPO_ROOT);
  }

  let dir = process.cwd();
  for (let i = 0; i < 10; i += 1) {
    if (existsSync(path.join(dir, "app", "mcp", "__main__.py"))) {
      return dir;
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      break;
    }
    dir = parent;
  }

  const fromPackage = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
  if (existsSync(path.join(fromPackage, "app", "mcp", "__main__.py"))) {
    return fromPackage;
  }

  console.error(
    "Cubiczan Autonomous Business OS checkout not found. Set ABOS_REPO_ROOT to the repo root, or run from the checkout.",
  );
  process.exit(2);
}

const repoRoot = findRepoRoot();
const python = process.env.ABOS_PYTHON || "python3";
const pythonPath = [repoRoot, process.env.PYTHONPATH].filter(Boolean).join(path.delimiter);

const child = spawn(python, ["-m", "app.mcp"], {
  cwd: repoRoot,
  stdio: "inherit",
  env: {
    ...process.env,
    PYTHONPATH: pythonPath,
  },
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
