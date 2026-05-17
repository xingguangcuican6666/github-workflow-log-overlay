export type LogDiff = {
  reset: boolean;
  added: string[];
};

export function splitLog(log: string): string[] {
  if (!log.trim()) return [];
  return log.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
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

