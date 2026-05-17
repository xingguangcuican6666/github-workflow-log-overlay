export type LogDiff = {
  reset: boolean;
  added: string[];
};

const ANSI_CONTROL_SEQUENCE_PATTERN =
  // OSC must be matched before generic single-character ESC sequences.
  /\x1B(?:\][\s\S]*?(?:\x07|\x1B\\)|\[[0-?]*[ -/]*[@-~]|[@-Z\\-_])|\u009B[0-?]*[ -/]*[@-~]/g;

export function stripControlSequences(value: string): string {
  return value.replace(ANSI_CONTROL_SEQUENCE_PATTERN, "");
}

export function splitLog(log: string): string[] {
  if (!log.trim()) return [];
  return stripControlSequences(log)
    .replace(/\r\n/g, "\n")
    .replace(/\r/g, "\n")
    .split("\n");
}

export function diffLogLines(previous: string[], next: string[]): LogDiff {
  if (next.length < previous.length) {
    return { reset: true, added: next };
  }

  for (let index = 0; index < previous.length; index += 1) {
    if (previous[index] !== next[index]) {
      return { reset: true, added: next };
    }
  }

  return { reset: false, added: next.slice(previous.length) };
}
