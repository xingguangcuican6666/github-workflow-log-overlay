import { describe, expect, it } from "vitest";
import { diffLogLines, splitLog } from "./logDiff";

describe("splitLog", () => {
  it("normalizes line endings", () => {
    expect(splitLog("one\r\ntwo\rthree")).toEqual(["one", "two", "three"]);
  });

  it("returns no lines for empty output", () => {
    expect(splitLog(" \n\t")).toEqual([]);
  });

  it("strips ANSI and OSC control sequences", () => {
    expect(splitLog("\u001b[32mok\u001b[0m\n\u001b]8;;https://example.com\u0007link\u001b]8;;\u0007")).toEqual([
      "ok",
      "link"
    ]);
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
