import test from "node:test";
import assert from "node:assert/strict";

import {
  parseRepositorySlugFromRemote,
  resolveSiteBuildContext,
} from "./repository-context.mjs";

test("parses GitHub repository slug from SSH remotes", () => {
  assert.equal(
    parseRepositorySlugFromRemote("git@github.com:jasperan/zerooraclaw.git"),
    "jasperan/zerooraclaw"
  );
});

test("parses GitHub repository slug from HTTPS remotes", () => {
  assert.equal(
    parseRepositorySlugFromRemote("https://github.com/zeroclaw-labs/zeroclaw.git"),
    "zeroclaw-labs/zeroclaw"
  );
});

test("resolves project Pages base paths from the current repository slug", () => {
  const context = resolveSiteBuildContext({
    githubRepository: "jasperan/zerooraclaw",
  });

  assert.equal(context.repositorySlug, "jasperan/zerooraclaw");
  assert.equal(context.pagesBasePath, "/zerooraclaw/");
  assert.equal(
    context.sourceBlobBaseUrl,
    "https://github.com/jasperan/zerooraclaw/blob/main"
  );
});

test("prefers an explicit Pages base path when the workflow provides one", () => {
  const context = resolveSiteBuildContext({
    githubRepository: "jasperan/zerooraclaw",
    pagesBasePath: "/docs/",
  });

  assert.equal(context.pagesBasePath, "/docs/");
});

test("uses root base path for user Pages repositories", () => {
  const context = resolveSiteBuildContext({
    githubRepository: "jasperan/jasperan.github.io",
  });

  assert.equal(context.pagesBasePath, "/");
});

test("treats an explicit empty Pages base path as root", () => {
  const context = resolveSiteBuildContext({
    githubRepository: "jasperan/zerooraclaw",
    pagesBasePath: "",
  });

  assert.equal(context.pagesBasePath, "/");
});
