import { describe, expect, it } from "vitest";
import { diffLogLines, splitLog } from "./logDiff";

describe("splitLog", () => {
  it("normalizes line endings", () => {
    expect(splitLog("one\r\ntwo\rthree")).toEqual(["one", "two", "three"]);
  });

  it("returns no lines for empty output", () => {
    expect(splitLog(" \n\t")).toEqual([]);
  });
});

describe("diffLogLines", () => {
  it("returns only appended lines", () => {
    expect(diffLogLines(["one", "two"], ["one", "two", "three"])).toEqual({
      reset: false,
      added: ["three"]
    });
  });

  it("resets when log output is truncated", () => {
    expect(diffLogLines(["one", "two"], ["one"])).toEqual({
      reset: true,
      added: ["one"]
    });
  });

  it("resets when existing lines change", () => {
    expect(diffLogLines(["one", "two"], ["one", "changed", "three"])).toEqual({
      reset: true,
      added: ["one", "changed", "three"]
    });
  });
});

