import { execSync } from "node:child_process";

function trimToNull(value) {
  if (typeof value !== "string") {
    return null;
  }

  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

export function normalizeRepositorySlug(value) {
  const trimmed = trimToNull(value);
  if (!trimmed) {
    return null;
  }

  const normalized = trimmed
    .replace(/^https?:\/\/github\.com\//i, "")
    .replace(/^git@github\.com:/i, "")
    .replace(/\.git$/i, "")
    .replace(/^\/+/, "")
    .replace(/\/+$/, "");

  const match = /^([^/]+)\/([^/]+)$/.exec(normalized);
  if (!match) {
    return null;
  }

  return `${match[1]}/${match[2]}`;
}

export function parseRepositorySlugFromRemote(remoteUrl) {
  return normalizeRepositorySlug(remoteUrl);
}

export function readGitRemoteUrl(cwd) {
  try {
    const remoteUrl = execSync("git config --get remote.origin.url", {
      cwd,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });

    return trimToNull(remoteUrl);
  } catch {
    return null;
  }
}

export function normalizePagesBasePath(value) {
  if (typeof value !== "string") {
    return null;
  }

  const trimmed = value.trim();
  if (!trimmed || trimmed === "/") {
    return "/";
  }

  return `/${trimmed.replace(/^\/+/, "").replace(/\/+$/, "")}/`.replace(/\/+/g, "/");
}

export function derivePagesBasePath(repositorySlug) {
  const normalizedSlug = normalizeRepositorySlug(repositorySlug);
  if (!normalizedSlug) {
    return "/";
  }

  const [owner, repo] = normalizedSlug.split("/");
  if (owner && repo && repo.toLowerCase() === `${owner.toLowerCase()}.github.io`) {
    return "/";
  }

  return `/${repo}/`;
}

export function resolveSiteBuildContext({
  githubRepository,
  remoteUrl,
  pagesBasePath,
  branch = "main",
} = {}) {
  const repositorySlug =
    normalizeRepositorySlug(githubRepository) ?? parseRepositorySlugFromRemote(remoteUrl);
  const normalizedBranch = trimToNull(branch) ?? "main";
  const normalizedPagesBasePath =
    normalizePagesBasePath(pagesBasePath) ?? derivePagesBasePath(repositorySlug);
  const sourceBlobBaseUrl = repositorySlug
    ? `https://github.com/${repositorySlug}/blob/${normalizedBranch}`
    : null;

  return {
    repositorySlug,
    pagesBasePath: normalizedPagesBasePath,
    sourceBlobBaseUrl,
  };
}
