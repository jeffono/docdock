// Deletes this checkout's disposable work files and lists merged worktrees and branches.
// Removing those stays an explicit action: this script only reports them.
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dirname, "..");
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();

rmSync(join(root, ".tmp"), { recursive: true, force: true });
console.log("Deleted .tmp/");

const merged = git("branch", "--merged", "main", "--format=%(refname:short)")
  .split("\n")
  .filter((branch) => branch && branch !== "main");
const worktrees = git("worktree", "list", "--porcelain")
  .split("\n\n")
  .slice(1)
  .map((block) => ({
    path: block.match(/^worktree (.+)$/m)?.[1],
    branch: block.match(/^branch refs\/heads\/(.+)$/m)?.[1],
  }))
  .filter(({ branch }) => !branch || merged.includes(branch));
const remote = git("branch", "-r", "--merged", "main", "--format=%(refname:short)")
  .split("\n")
  .filter((branch) => branch.startsWith("origin/") && !["origin/main", "origin"].includes(branch));

if (!merged.length && !worktrees.length && !remote.length) console.log("Nothing merged to remove.");
for (const { path, branch } of worktrees)
  console.log(`git worktree remove ${path}  # ${branch ?? "detached: check before removing"}`);
for (const branch of merged) console.log(`git branch -d ${branch}`);
for (const branch of remote) console.log(`git push origin --delete ${branch.slice(7)}`);
