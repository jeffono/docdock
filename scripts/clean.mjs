// Deletes this checkout's disposable work files and lists merged worktrees and branches.
// Removing those stays an explicit action: this script only reports them.
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import { join, resolve } from "node:path";

const root = join(import.meta.dirname, "..");
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const lines = (text) => text.split("\n").filter(Boolean);
const merged = (commit) => {
  try {
    git("merge-base", "--is-ancestor", commit, "main");
    return true;
  } catch {
    return false;
  }
};

rmSync(join(root, ".tmp"), { recursive: true, force: true });
console.log("Deleted .tmp/");

const current = git("branch", "--show-current");
const here = resolve(git("rev-parse", "--show-toplevel"));
const branches = lines(git("branch", "--merged", "main", "--format=%(refname:short)")).filter(
  (branch) => branch !== "main" && branch !== current,
);
const worktrees = git("worktree", "list", "--porcelain")
  .split("\n\n")
  .map((block) => ({
    path: block.match(/^worktree (.+)$/m)?.[1],
    head: block.match(/^HEAD (.+)$/m)?.[1],
    branch: block.match(/^branch refs\/heads\/(.+)$/m)?.[1],
  }))
  .filter(({ path, head, branch }) => {
    if (!path || resolve(path) === here || branch === "main") return false;
    return branch ? branches.includes(branch) : merged(head);
  });
const remote = lines(git("branch", "-r", "--merged", "main", "--format=%(refname:short)"))
  .filter((branch) => branch.startsWith("origin/"))
  .map((branch) => branch.slice("origin/".length))
  .filter((branch) => !["main", "HEAD", "", current].includes(branch));

if (!branches.length && !worktrees.length && !remote.length)
  console.log("Nothing merged to remove.");
for (const { path, branch } of worktrees)
  console.log(`git worktree remove "${path}"  # ${branch ?? "detached, merged"}`);
for (const branch of branches) console.log(`git branch -d ${branch}`);
for (const branch of remote) console.log(`git push origin --delete ${branch}`);
