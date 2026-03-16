import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

import {
  readGitRemoteUrl,
  resolveSiteBuildContext,
} from "./scripts/repository-context.mjs";

const buildContext = resolveSiteBuildContext({
  githubRepository: process.env.GITHUB_REPOSITORY,
  remoteUrl: readGitRemoteUrl(process.cwd()),
  pagesBasePath: process.env.VITE_BASE_PATH,
});

const repositorySlug = buildContext.repositorySlug ?? "zeroclaw-labs/zeroclaw";
const repositoryUrl = `https://github.com/${repositorySlug}`;
const rawBaseUrl = `https://raw.githubusercontent.com/${repositorySlug}/main`;

export default defineConfig({
  base: buildContext.pagesBasePath,
  define: {
    __SITE_REPOSITORY_URL__: JSON.stringify(repositoryUrl),
    __SITE_SOURCE_BLOB_BASE_URL__: JSON.stringify(
      buildContext.sourceBlobBaseUrl ?? `${repositoryUrl}/blob/main`
    ),
    __SITE_RAW_BASE_URL__: JSON.stringify(rawBaseUrl),
  },
  plugins: [react()],
  build: {
    outDir: "../gh-pages",
    emptyOutDir: true,
  },
});
