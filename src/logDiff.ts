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

function commonTailWindowOverlap(previous: string[], next: string[]): number {
  const maxOverlap = Math.min(previous.length, next.length);
  const minUsefulOverlap = Math.min(20, maxOverlap);

  for (let size = maxOverlap; size >= minUsefulOverlap; size -= 1) {
    let matches = true;

    for (let index = 0; index < size; index += 1) {
      if (previous[previous.length - size + index] !== next[index]) {
        matches = false;
        break;
      }
    }

    if (matches) return size;
  }

  return 0;
}

export function diffLogLines(previous: string[], next: string[]): LogDiff {
  if (next.length < previous.length) {
    const overlap = commonTailWindowOverlap(previous, next);
    return overlap ? { reset: false, added: next.slice(overlap) } : { reset: true, added: next };
  }

  for (let index = 0; index < previous.length; index += 1) {
    if (previous[index] !== next[index]) {
      const overlap = commonTailWindowOverlap(previous, next);
      return overlap ? { reset: false, added: next.slice(overlap) } : { reset: true, added: next };
    }
  }

  return { reset: false, added: next.slice(previous.length) };
}
