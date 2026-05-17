import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { diffLogLines, splitLog } from "./logDiff";

type GhStatus = {
  installed: boolean;
  version?: string;
  authenticated: boolean;
  authMessage?: string;
};

type RunInfo = {
  databaseId: number;
  status: string;
  conclusion?: string | null;
  displayTitle?: string | null;
  headBranch?: string | null;
  event?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  url?: string | null;
  workflowName?: string | null;
};

type WorkflowLogResponse = {
  run?: RunInfo | null;
  log: string;
  warning?: string | null;
};

type Settings = {
  repo: string;
  workflow: string;
  branch: string;
  refreshSeconds: number;
  opacity: number;
};

type FetchState = "idle" | "loading" | "ok" | "error";

const STORAGE_KEY = "github-workflow-log-overlay.settings";
const MAX_VISIBLE_LINES = 900;
const MAX_QUEUE_LINES = 1200;
const STREAM_INTERVAL_MS = 80;
const DEFAULT_SETTINGS: Settings = {
  repo: "",
  workflow: "",
  branch: "",
  refreshSeconds: 4,
  opacity: 72
};

function loadSettings(): Settings {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (!saved) return DEFAULT_SETTINGS;
    return { ...DEFAULT_SETTINGS, ...JSON.parse(saved) };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function clampRefresh(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS.refreshSeconds;
  return Math.min(5, Math.max(3, value));
}

function statusLabel(run?: RunInfo | null): string {
  if (!run) return "No run";
  if (run.status === "completed") return run.conclusion ?? "completed";
  return run.status;
}

function statusTone(run?: RunInfo | null): string {
  const label = statusLabel(run);
  if (label === "success") return "success";
  if (["failure", "cancelled", "timed_out"].includes(label)) return "danger";
  if (["in_progress", "queued", "requested", "waiting", "pending"].includes(label)) return "active";
  return "neutral";
}

function formatWhen(value?: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export default function App() {
  const [settings, setSettings] = useState<Settings>(() => loadSettings());
  const [ghStatus, setGhStatus] = useState<GhStatus | null>(null);
  const [fetchState, setFetchState] = useState<FetchState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [run, setRun] = useState<RunInfo | null>(null);
  const [visibleLines, setVisibleLines] = useState<string[]>([]);
  const [queuedLines, setQueuedLines] = useState<string[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(true);
  const [lastFetchAt, setLastFetchAt] = useState<Date | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const lastRunId = useRef<number | null>(null);
  const rawLinesRef = useRef<string[]>([]);
  const logPaneRef = useRef<HTMLElement | null>(null);
  const scrollFrameRef = useRef<number | null>(null);
  const fetchingRef = useRef(false);

  const canFetch = settings.repo.trim().includes("/") && settings.workflow.trim().length > 0;

  const appStyle = useMemo(
    () => ({ "--overlay-opacity": `${settings.opacity / 100}` }) as React.CSSProperties,
    [settings.opacity]
  );

  useEffect(() => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ ...settings, refreshSeconds: clampRefresh(settings.refreshSeconds) })
    );
  }, [settings]);

  useEffect(() => {
    invoke<GhStatus>("check_gh")
      .then(setGhStatus)
      .catch((caught) => {
        setGhStatus({
          installed: false,
          authenticated: false,
          authMessage: String(caught)
        });
      });
  }, []);

  useEffect(() => {
    if (!queuedLines.length) return;

    const timer = window.setInterval(() => {
      setQueuedLines((currentQueue) => {
        if (!currentQueue.length) {
          window.clearInterval(timer);
          return currentQueue;
        }

        const batchSize = currentQueue.length > 600 ? 120 : currentQueue.length > 120 ? 60 : 24;
        const nextBatch = currentQueue.slice(0, batchSize);
        setVisibleLines((currentVisible) => [...currentVisible, ...nextBatch].slice(-MAX_VISIBLE_LINES));
        return currentQueue.slice(batchSize);
      });
    }, STREAM_INTERVAL_MS);

    return () => window.clearInterval(timer);
  }, [queuedLines.length]);

  useEffect(() => {
    if (!autoScroll) return;

    if (scrollFrameRef.current !== null) {
      window.cancelAnimationFrame(scrollFrameRef.current);
    }

    scrollFrameRef.current = window.requestAnimationFrame(() => {
      const logPane = logPaneRef.current;
      if (logPane) {
        logPane.scrollTop = logPane.scrollHeight;
      }
    });

    return () => {
      if (scrollFrameRef.current !== null) {
        window.cancelAnimationFrame(scrollFrameRef.current);
        scrollFrameRef.current = null;
      }
    };
  }, [visibleLines.length, autoScroll]);

  const fetchLogs = async () => {
    if (!canFetch || fetchingRef.current) return;
    fetchingRef.current = true;
    setFetchState("loading");
    setError(null);

    try {
      const response = await invoke<WorkflowLogResponse>("fetch_workflow_log", {
        request: {
          repo: settings.repo.trim(),
          workflow: settings.workflow.trim(),
          branch: settings.branch.trim() || null
        }
      });

      const nextLines = splitLog(response.log);
      const nextRunId = response.run?.databaseId ?? null;
      const runChanged = nextRunId !== lastRunId.current;
      const diff = runChanged
        ? { reset: true, added: nextLines }
        : diffLogLines(rawLinesRef.current, nextLines);

      if (diff.reset) {
        setVisibleLines([]);
        setQueuedLines(diff.added.slice(-MAX_VISIBLE_LINES));
      } else if (diff.added.length) {
        setQueuedLines((currentQueue) => [...currentQueue, ...diff.added].slice(-MAX_QUEUE_LINES));
      }

      lastRunId.current = nextRunId;
      rawLinesRef.current = nextLines;
      setRun(response.run ?? null);
      setWarning(response.warning ?? null);
      setLastFetchAt(new Date());
      setFetchState("ok");
    } catch (caught) {
      setError(String(caught));
      setFetchState("error");
    } finally {
      fetchingRef.current = false;
    }
  };

  useEffect(() => {
    if (!canFetch) return;
    void fetchLogs();

    const refreshMs = clampRefresh(settings.refreshSeconds) * 1000;
    const timer = window.setInterval(() => {
      void fetchLogs();
    }, refreshMs);

    return () => window.clearInterval(timer);
    // fetchLogs reads the latest log snapshot through refs, so polling does not reset on every response.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [canFetch, settings.repo, settings.workflow, settings.branch, settings.refreshSeconds]);

  const updateSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((current) => ({
      ...current,
      [key]: key === "refreshSeconds" ? clampRefresh(Number(value)) : value
    }));
  };

  const clearLog = () => {
    lastRunId.current = null;
    rawLinesRef.current = [];
    setVisibleLines([]);
    setQueuedLines([]);
    setRun(null);
    setWarning(null);
    setError(null);
  };

  const closeWindow = () => {
    void getCurrentWindow().close();
  };

  const minimizeWindow = () => {
    void getCurrentWindow().minimize();
  };

  return (
    <main className="app-shell" style={appStyle}>
      <section className="overlay-card">
        <header className="titlebar" data-tauri-drag-region>
          <div className="brand" data-tauri-drag-region>
            <span className="brand-mark">GH</span>
            <div data-tauri-drag-region>
              <strong>Workflow Log Overlay</strong>
              <span>{settings.repo || "Configure a repository"}</span>
            </div>
          </div>

          <div className="window-actions">
            <button type="button" onClick={() => setSettingsOpen((open) => !open)}>
              {settingsOpen ? "Hide" : "Config"}
            </button>
            <button type="button" onClick={minimizeWindow} aria-label="Minimize">
              _
            </button>
            <button type="button" onClick={closeWindow} aria-label="Close">
              x
            </button>
          </div>
        </header>

        <div className="status-row">
          <span className={`status-pill ${statusTone(run)}`}>{statusLabel(run)}</span>
          <span>{run?.displayTitle || run?.workflowName || settings.workflow || "Waiting for configuration"}</span>
          {run?.headBranch ? <span className="muted">branch {run.headBranch}</span> : null}
          {lastFetchAt ? <span className="muted">updated {lastFetchAt.toLocaleTimeString()}</span> : null}
        </div>

        {settingsOpen ? (
          <form className="settings-panel" onSubmit={(event) => event.preventDefault()}>
            <label>
              Repository
              <input
                placeholder="owner/repo"
                value={settings.repo}
                onChange={(event) => updateSetting("repo", event.target.value)}
              />
            </label>
            <label>
              Workflow
              <input
                placeholder="build.yml or Build"
                value={settings.workflow}
                onChange={(event) => updateSetting("workflow", event.target.value)}
              />
            </label>
            <label>
              Branch
              <input
                placeholder="optional"
                value={settings.branch}
                onChange={(event) => updateSetting("branch", event.target.value)}
              />
            </label>
            <label>
              Refresh
              <input
                type="number"
                min={3}
                max={5}
                step={1}
                value={settings.refreshSeconds}
                onChange={(event) => updateSetting("refreshSeconds", Number(event.target.value))}
              />
            </label>
            <label>
              Opacity
              <input
                type="range"
                min={45}
                max={92}
                value={settings.opacity}
                onChange={(event) => updateSetting("opacity", Number(event.target.value))}
              />
            </label>
            <div className="button-row">
              <button type="button" onClick={() => void fetchLogs()} disabled={!canFetch || fetchState === "loading"}>
                {fetchState === "loading" ? "Refreshing" : "Refresh now"}
              </button>
              <button type="button" onClick={clearLog}>
                Clear
              </button>
              <button type="button" onClick={() => setAutoScroll((enabled) => !enabled)}>
                Autoscroll {autoScroll ? "on" : "off"}
              </button>
            </div>
          </form>
        ) : null}

        <div className="health-row">
          <span className={ghStatus?.installed ? "ok" : "bad"}>
            gh {ghStatus?.installed ? ghStatus.version ?? "installed" : "missing"}
          </span>
          <span className={ghStatus?.authenticated ? "ok" : "warn"}>
            {ghStatus?.authenticated ? "authenticated" : "gh auth not confirmed"}
          </span>
          {run?.url ? (
            <span className="muted" title={run.url}>
              {formatWhen(run.updatedAt)}
            </span>
          ) : null}
        </div>

        {warning ? <div className="message warning">{warning}</div> : null}
        {error ? <div className="message error">{error}</div> : null}
        {!canFetch ? <div className="message info">Set repository and workflow to start polling.</div> : null}

        <section className="log-pane" ref={logPaneRef} aria-label="Workflow logs">
          {visibleLines.length ? (
            visibleLines.map((line, index) => (
              <div className="log-line" key={`${index}-${line.slice(0, 24)}`}>
                <span className="line-number">{index + 1}</span>
                <span className="line-text">{line || " "}</span>
              </div>
            ))
          ) : (
            <div className="empty-log">No log output yet.</div>
          )}
        </section>
      </section>
    </main>
  );
}
