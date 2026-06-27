/**
 * Smoke tests for path <-> file URI conversion. Run with `bun test`.
 * These verify the contract documented in `fileUriToPath` / `pathToFileUri`.
 */

import { test, expect } from "bun:test";
import {
  fileUriToPath,
  pathToFileUri,
  basename,
  dirname,
  joinPath,
  extname,
  stripExt,
} from "./utils";

test("fileUriToPath: Unix path", () => {
  expect(fileUriToPath("file:///home/user/script.sk")).toBe("/home/user/script.sk");
});

test("fileUriToPath: Windows path", () => {
  expect(fileUriToPath("file:///C:/Users/me/script.sk")).toBe("C:\\Users\\me\\script.sk");
});

test("fileUriToPath: percent-encoded spaces", () => {
  expect(fileUriToPath("file:///home/user/my%20scripts/script.sk")).toBe(
    "/home/user/my scripts/script.sk",
  );
});

test("fileUriToPath: non-file URI returns null", () => {
  expect(fileUriToPath("https://example.com/script.sk")).toBeNull();
  expect(fileUriToPath("untitled-1.sk")).toBeNull();
});

test("pathToFileUri: Unix path round-trips through fileUriToPath", () => {
  const path = "/home/user/script.sk";
  const uri = pathToFileUri(path);
  expect(uri).toBe("file:///home/user/script.sk");
  expect(fileUriToPath(uri)).toBe(path);
});

test("pathToFileUri: Windows path round-trips", () => {
  const path = "C:\\Users\\me\\script.sk";
  const uri = pathToFileUri(path);
  expect(fileUriToPath(uri)).toBe(path);
});

test("basename: extracts file name", () => {
  expect(basename("/home/user/script.sk")).toBe("script.sk");
  expect(basename("script.sk")).toBe("script.sk");
  expect(basename("C:\\Users\\me\\script.sk")).toBe("script.sk");
});

test("dirname: extracts directory", () => {
  expect(dirname("/home/user/script.sk")).toBe("/home/user");
  expect(dirname("/home/user/")).toBe("/home/user");
  expect(dirname("script.sk")).toBe("");
});

test("extname: extracts lowercase extension", () => {
  expect(extname("script.sk")).toBe("sk");
  expect(extname("script.SK")).toBe("sk");
  expect(extname("script")).toBe("");
  expect(extname("path/to/script.sk")).toBe("sk");
});

test("stripExt: removes extension", () => {
  expect(stripExt("script.sk")).toBe("script");
  expect(stripExt("script")).toBe("script");
  expect(stripExt("a.b.c")).toBe("a.b");
});

test("joinPath: joins segments", () => {
  expect(joinPath("/home", "user", "script.sk")).toBe("/home/user/script.sk");
  expect(joinPath("/home/", "/user/", "script.sk")).toBe("/home/user/script.sk");
  expect(joinPath("C:\\Users", "me", "script.sk")).toBe("C:/Users/me/script.sk");
});
