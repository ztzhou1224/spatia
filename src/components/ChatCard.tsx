import { useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { isTauri } from "../lib/tauri";
import { useAppStore } from "../lib/appStore";
import type { ResultRows, WidgetType } from "../lib/appStore";
import type { MapViewHandle } from "./MapView";
import { executeMapActions } from "../lib/mapActions";

type Props = {
  mapViewRef: React.RefObject<MapViewHandle | null>;
  panelWidth?: number;
};

type SqlEditState = {
  expanded: boolean;
  editing: boolean;
  editedSql: string;
  running: boolean;
  error: string | null;
};

const DEFAULT_SQL_STATE: SqlEditState = {
  expanded: false,
  editing: false,
  editedSql: "",
  running: false,
  error: null,
};

// Map visualization types that go directly on the map — no widget
const MAP_VIZ_TYPES = new Set(["scatter", "heatmap", "hexbin"]);

function toWidgetType(vizType: string): WidgetType | null {
  switch (vizType) {
    case "table": return "table";
    case "bar_chart": return "bar_chart";
    case "pie_chart": return "pie_chart";
    case "histogram": return "histogram";
    default:
      if (!MAP_VIZ_TYPES.has(vizType) && vizType !== "") return "table";
      return null;
  }
}

const ROWS_PER_PAGE = 20;

function ResultTable({ resultRows, totalCount }: { resultRows: ResultRows; totalCount?: number | null }) {
  const { columns, rows, truncated } = resultRows;
  const [currentPage, setCurrentPage] = useState(0);

  if (columns.length === 0) return null;

  const totalPages = Math.ceil(rows.length / ROWS_PER_PAGE);
  const pageRows = rows.slice(currentPage * ROWS_PER_PAGE, (currentPage + 1) * ROWS_PER_PAGE);

  return (
    <div className="mt-2">
      <div className="overflow-x-auto max-w-full">
        <table className="min-w-full border-collapse text-[11px] font-mono whitespace-nowrap">
          <thead>
            <tr>
              {columns.map((col) => (
                <th
                  key={col}
                  className="px-2 py-1 text-left bg-secondary border-b border-border font-semibold"
                >
                  {col}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {pageRows.map((row, ri) => (
              <tr
                key={ri}
                className={ri % 2 === 0 ? "" : "bg-secondary/50"}
              >
                {row.map((cell, ci) => (
                  <td
                    key={ci}
                    className="px-2 py-0.5 border-b border-border/50 max-w-[200px] overflow-hidden text-ellipsis"
                    title={cell ?? "null"}
                  >
                    {cell ?? <em className="text-muted-foreground">null</em>}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {rows.length > ROWS_PER_PAGE && (
        <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
          <button
            className="px-1.5 py-0.5 border border-border rounded disabled:opacity-40 hover:bg-secondary"
            onClick={() => setCurrentPage((p) => Math.max(0, p - 1))}
            disabled={currentPage === 0}
          >
            &lt; Prev
          </button>
          <span>Page {currentPage + 1} of {totalPages}</span>
          <button
            className="px-1.5 py-0.5 border border-border rounded disabled:opacity-40 hover:bg-secondary"
            onClick={() => setCurrentPage((p) => Math.min(totalPages - 1, p + 1))}
            disabled={currentPage === totalPages - 1}
          >
            Next &gt;
          </button>
        </div>
      )}
      {truncated && (
        <p className="text-xs text-muted-foreground mt-1">
          {totalCount != null
            ? `Showing ${rows.length} of ${totalCount.toLocaleString()} rows`
            : `Showing first ${rows.length} rows only.`}
        </p>
      )}
    </div>
  );
}

export function ChatCard({ mapViewRef, panelWidth = 300 }: Props) {
  const [input, setInput] = useState("");
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [sqlStates, setSqlStates] = useState<Record<number, SqlEditState>>({});

  const messages = useAppStore((s) => s.chatMessages);
  const addMessage = useAppStore((s) => s.addMessage);
  const clearMessages = useAppStore((s) => s.clearMessages);
  const setAnalysisGeoJson = useAppStore((s) => s.setAnalysisGeoJson);
  const setVisualizationType = useAppStore((s) => s.setVisualizationType);
  const apiConfig = useAppStore((s) => s.apiConfig);
  const selectedTablesForChat = useAppStore((s) => s.selectedTablesForChat);
  const logPath = useAppStore((s) => s.logPath);
  const tables = useAppStore((s) => s.tables);
  const activeWidget = useAppStore((s) => s.activeWidget);
  const setActiveWidget = useAppStore((s) => s.setActiveWidget);
  const domainConfig = useAppStore((s) => s.domainConfig);
  const analysisGeoJson = useAppStore((s) => s.analysisGeoJson);
  const setAnalysisTotalCount = useAppStore((s) => s.setAnalysisTotalCount);

  const tableNames = Array.from(selectedTablesForChat).sort();

  const updateMessage = useCallback((index: number, patch: Partial<{ sql: string; resultRows: ResultRows; rowCount: number }>) => {
    useAppStore.setState((state) => ({
      chatMessages: state.chatMessages.map((m, i) =>
        i === index ? { ...m, ...patch } : m
      ),
    }));
  }, []);

  function getSqlState(index: number): SqlEditState {
    return sqlStates[index] ?? DEFAULT_SQL_STATE;
  }

  function patchSql(index: number, patch: Partial<SqlEditState>) {
    setSqlStates((prev) => ({
      ...prev,
      [index]: { ...(prev[index] ?? DEFAULT_SQL_STATE), ...patch },
    }));
  }

  async function handleRunEditedSql(msgIndex: number, editedSql: string) {
    if (!isTauri()) return;
    patchSql(msgIndex, { running: true, error: null });
    try {
      const raw = await invoke<string>("execute_analysis_sql", { sql: editedSql });
      const result = JSON.parse(raw) as {
        status: string;
        row_count: number;
        total_count: number;
        geojson: unknown;
        tabular: { columns: string[]; rows: (string | null)[][]; truncated: boolean };
      };
      updateMessage(msgIndex, {
        sql: editedSql,
        resultRows: {
          columns: result.tabular.columns,
          rows: result.tabular.rows,
          truncated: result.tabular.truncated,
        },
        rowCount: result.row_count,
      });
      setAnalysisGeoJson(result.geojson);
      setAnalysisTotalCount(result.total_count ?? null);
      patchSql(msgIndex, { editing: false, running: false, error: null });
    } catch (err) {
      patchSql(msgIndex, { running: false, error: String(err) });
    }
  }

  async function doSend(text: string) {
    if (!text || loading) return;
    setExpanded(true);
    setLoading(true);
    setError(null);
    setInput("");
    addMessage({ role: "user", content: text });

    const conversationHistory = messages.map((m) => ({ role: m.role, content: m.content }));

    if (!isTauri()) {
      addMessage({ role: "assistant", content: "Demo mode: AI chat requires Tauri backend. Run \`pnpm tauri dev\`." });
      setLoading(false);
      return;
    }

    try {
      const raw = await invoke<string>("chat_turn", { tableNames, userMessage: text, conversationHistory });
      const result = JSON.parse(raw) as {
        message: string; sql?: string; geojson?: unknown; map_actions: unknown[];
        row_count?: number; total_count?: number; result_rows?: ResultRows;
        visualization_type?: string; retry_attempted?: boolean;
      };

      const vizType = result.visualization_type ?? "";
      const widgetType = toWidgetType(vizType);
      const hasTableData = result.result_rows != null && result.result_rows.columns.length > 0 && result.result_rows.rows.length > 0;
      const sendToWidget = hasTableData && widgetType !== null;

      addMessage({
        role: "assistant", content: result.message, sql: result.sql ?? undefined,
        rowCount: result.row_count ?? undefined,
        resultRows: sendToWidget ? undefined : (result.result_rows ?? undefined),
        retryAttempted: result.retry_attempted ?? false,
      });

      if (sendToWidget && result.result_rows) {
        setActiveWidget({ type: widgetType, title: result.message, data: result.result_rows });
      }

      setAnalysisTotalCount(result.total_count ?? null);

      if (result.geojson && MAP_VIZ_TYPES.has(vizType)) {
        setAnalysisGeoJson(result.geojson);
        setVisualizationType(vizType);
      } else if (result.geojson && !widgetType) {
        setAnalysisGeoJson(result.geojson);
        setVisualizationType(result.visualization_type ?? "scatter");
      }

      if (result.map_actions?.length) {
        const map = mapViewRef.current?.getMap();
        if (map) executeMapActions(map, result.map_actions);
      }
    } catch (err) {
      setError(String(err));
    }

    setLoading(false);
    setTimeout(() => messagesEndRef.current?.scrollIntoView({ behavior: "smooth" }), 100);
  }

  function handleSend() { void doSend(input.trim()); }

  async function handleExportGeoJson() {
    if (!isTauri()) return;
    try {
      const filePath = await save({ defaultPath: "analysis_result.geojson", filters: [{ name: "GeoJSON", extensions: ["geojson"] }] });
      if (filePath) await invoke("export_analysis_geojson", { filePath });
    } catch { /* ignore */ }
  }

  const hasAnalysisFeatures = ((analysisGeoJson as { features?: unknown[] })?.features ?? []).length > 0;

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); }
  }

  const suggestions = tableNames.length > 0 ? [
    `Show all rows from ${tableNames[0]} on the map`,
    `Summarize ${tableNames[0]}`,
    `Show a heatmap of ${tableNames[0]}`,
    `Find the top 10 records in ${tableNames[0]}`,
  ] : [];

  return (
    <div
      className="absolute bottom-4 left-4 z-10 w-[420px]"
      style={{ maxWidth: `calc(100vw - ${panelWidth + 40}px)`, transition: "max-width 250ms ease" }}
    >
      {expanded && (
        <Card className="mb-2 p-3" style={{ boxShadow: "0 8px 32px rgba(0,0,0,0.5), 0 2px 8px rgba(0,0,0,0.3)" }}>
          <div className="flex flex-col gap-2 min-h-0 overflow-y-auto" style={{ maxHeight: "372px" }}>
            {apiConfig !== null && !apiConfig.gemini && (
              <div className="rounded-lg border border-warning/30 bg-warning/10 p-2">
                <p className="text-xs text-warning">
                  AI analysis requires a Gemini API key. Set the{" "}
                  <strong>SPATIA_GEMINI_API_KEY</strong> environment variable and restart the app.
                </p>
              </div>
            )}

            {messages.length === 0 && tables.length === 0 && (
              <p className="text-xs text-muted-foreground">Upload data first, then ask questions about it here.</p>
            )}

            {messages.length === 0 && tables.length > 0 && tableNames.length === 0 && (
              <p className="text-xs text-muted-foreground">Check the boxes next to your tables on the right to add them to context, then ask a question.</p>
            )}

            {messages.length === 0 && tableNames.length > 0 && (
              <div className="flex flex-col gap-2">
                <p className="text-xs text-muted-foreground">Try one of these, or ask your own question:</p>
                <div className="flex flex-wrap gap-1.5">
                  {suggestions.map((s) => (
                    <button key={s} onClick={() => void doSend(s)}
                      className="text-xs rounded-full px-3 py-1 border border-border/50 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors cursor-pointer">
                      {s}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {messages.map((msg, i) => {
              const st = msg.sql ? getSqlState(i) : null;
              return (
                <div key={i} className="rounded-lg p-2"
                  style={msg.role === "user"
                    ? { background: "rgba(139,92,246,0.10)", border: "1px solid rgba(139,92,246,0.15)" }
                    : { background: "rgba(148,163,184,0.06)", border: "1px solid rgba(148,163,184,0.08)" }
                  }>
                  <p className="text-xs text-muted-foreground mb-0.5" style={msg.role === "user" ? { textAlign: "right" } : undefined}>
                    {msg.role === "user" ? "You" : domainConfig.assistant_name}
                  </p>
                  <p className="text-sm whitespace-pre-wrap">{msg.content}</p>

                  {msg.sql && st && (
                    <div className="mt-1.5">
                      <div className="flex items-center gap-2 mb-0.5">
                        <button onClick={() => patchSql(i, { expanded: !st.expanded, editedSql: st.editedSql || msg.sql! })}
                          className="flex items-center gap-1 text-[10px] font-mono text-muted-foreground uppercase tracking-wider hover:text-foreground transition-colors">
                          <span style={{ display: "inline-block", transform: st.expanded ? "rotate(90deg)" : "rotate(0deg)", transition: "transform 150ms" }}>&#9654;</span>
                          SQL
                        </button>
                        {msg.retryAttempted && <span className="text-[10px] text-muted-foreground">Auto-corrected</span>}
                        {st.expanded && !st.editing && (
                          <button onClick={() => patchSql(i, { editing: true, editedSql: msg.sql! })}
                            className="text-[10px] text-muted-foreground hover:text-foreground transition-colors ml-auto">
                            Edit &amp; Run
                          </button>
                        )}
                      </div>
                      {st.expanded && (st.editing ? (
                        <div className="flex flex-col gap-1">
                          <textarea value={st.editedSql} onChange={(e) => patchSql(i, { editedSql: e.target.value })}
                            className="p-1.5 bg-secondary text-[11px] font-mono whitespace-pre overflow-x-auto border border-border rounded-lg resize-y min-h-[60px] w-full focus:outline-none focus:ring-1 focus:ring-ring"
                            rows={4} />
                          <div className="flex gap-1.5">
                            <button onClick={() => void handleRunEditedSql(i, st.editedSql)} disabled={st.running}
                              className="text-[10px] px-2 py-0.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50">
                              {st.running ? "Running..." : "Run"}
                            </button>
                            <button onClick={() => patchSql(i, { editing: false, editedSql: msg.sql!, error: null })}
                              className="text-[10px] px-2 py-0.5 rounded border border-border hover:bg-secondary">
                              Cancel
                            </button>
                          </div>
                          {st.error && <p className="text-[10px] text-destructive">{st.error}</p>}
                        </div>
                      ) : (
                        <div className="p-1.5 bg-secondary text-[11px] font-mono whitespace-pre-wrap overflow-x-auto" style={{ borderRadius: "0.5rem" }}>
                          {msg.sql}
                        </div>
                      ))}
                    </div>
                  )}

                  {msg.resultRows && msg.resultRows.columns.length > 0 ? (
                    <ResultTable resultRows={msg.resultRows} totalCount={useAppStore.getState().analysisTotalCount} />
                  ) : msg.rowCount != null ? (
                    <p className="text-xs text-muted-foreground">{msg.rowCount} row(s) returned</p>
                  ) : null}

                  {msg.role === "assistant" && !msg.resultRows && msg.rowCount == null && activeWidget && i === messages.length - 1 && activeWidget.title === msg.content && (
                    <p className="text-xs text-muted-foreground mt-1">Results shown in widget below.</p>
                  )}
                </div>
              );
            })}
            <div ref={messagesEndRef} />
          </div>
        </Card>
      )}

      {tableNames.length > 0 && (
        <div className="flex gap-1 flex-wrap items-center py-0.5">
          <span className="text-xs text-muted-foreground">Context:</span>
          {tableNames.map((name) => (<Badge key={name} variant="info">{name}</Badge>))}
        </div>
      )}

      <div className="flex gap-2 items-center glass-panel p-2 rounded-lg border border-border"
        style={{ boxShadow: "0 4px 20px rgba(0,0,0,0.4), 0 1px 4px rgba(0,0,0,0.3)" }}>
        <Input value={input}
          onChange={(e) => { setInput(e.target.value); if (!expanded && e.target.value) setExpanded(true); }}
          onKeyDown={handleKeyDown} onFocus={() => setExpanded(true)}
          placeholder={tables.length === 0 ? domainConfig.ui_config.placeholder_no_data : tableNames.length === 0 ? domainConfig.ui_config.placeholder_no_selection : domainConfig.ui_config.placeholder_ready}
          className="flex-1" disabled={loading} />
        {loading ? (<Spinner size="md" />) : (
          <Button size="icon" onClick={handleSend} disabled={!input.trim() || tableNames.length === 0} title="Send message" className="w-8 h-8 shrink-0">
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M8 13V3M8 3l-4.5 4.5M8 3l4.5 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </Button>
        )}
        {expanded && (
          <button onClick={() => setExpanded(false)} title="Collapse chat"
            className="w-7 h-7 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors shrink-0">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M3 5l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        )}
        {isTauri() && hasAnalysisFeatures && (
          <button onClick={() => void handleExportGeoJson()} title="Export GeoJSON"
            className="w-7 h-7 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors shrink-0">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M7 1.5v8M7 9.5l-3-3M7 9.5l3-3M2 11.5h10" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        )}
        {messages.length > 0 && (
          <button onClick={() => { clearMessages(); setExpanded(false); }} title="New conversation"
            className="w-7 h-7 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors shrink-0">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M8.5 2.5l3 3M2 9l6.5-6.5 3 3L5 12H2V9z" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
              <path d="M2 12.5h10" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
            </svg>
          </button>
        )}
      </div>

      {error && (
        <div className="mt-1">
          <p className="text-xs text-destructive">{error}</p>
          {logPath && <p className="text-xs text-muted-foreground mt-1">For details, see log: <code className="font-mono text-[10px]">{logPath}</code></p>}
        </div>
      )}
    </div>
  );
}
