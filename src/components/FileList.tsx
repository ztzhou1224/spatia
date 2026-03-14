import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { isTauri } from "../lib/tauri";
import { useAppStore, type TableInfo, type GeocodeStats } from "../lib/appStore";

async function loadTableGeoJson(
  tableName: string,
  setTableGeoJson: (name: string, geojson: unknown) => void
): Promise<void> {
  try {
    const raw = await invoke<string>("table_to_geojson", { tableName });
    setTableGeoJson(tableName, JSON.parse(raw));
  } catch {
    // Non-fatal: table may not have geocoded columns yet
  }
}

function sanitizeTableName(filename: string): string {
  const base = filename.replace(/.*[\\/]/, "").replace(/\.[^.]+$/, "");
  let name = base.toLowerCase().replace(/[^a-z0-9]+/g, "_");
  name = name.replace(/^[0-9]+/, "");
  if (!name || name === "_") name = "table_" + Date.now();
  return name;
}

function statusVariant(status: TableInfo["status"]): "info" | "success" | "destructive" | "warning" | "secondary" {
  switch (status) {
    case "ingesting":
    case "cleaning":
    case "detecting":
    case "geocoding":
      return "info";
    case "ready":
      return "warning";
    case "done":
      return "success";
    case "error":
      return "destructive";
    default:
      return "secondary";
  }
}

function statusIcon(status: TableInfo["status"]): string {
  switch (status) {
    case "ingesting":
    case "cleaning":
    case "detecting":
    case "geocoding":
      return "⟳";
    case "done":
      return "✓";
    case "error":
      return "✕";
    case "ready":
      return "◉";
    default:
      return "";
  }
}

function isActive(status: TableInfo["status"]): boolean {
  return ["ingesting", "cleaning", "detecting", "geocoding"].includes(status);
}

function GeocodeStatsSummary({ stats }: { stats: GeocodeStats }) {
  const parts: string[] = [];
  if (stats.by_source.cache > 0) parts.push(`${stats.by_source.cache} cached`);
  if (stats.by_source.overture_fuzzy > 0) parts.push(`${stats.by_source.overture_fuzzy} local match`);
  if (stats.by_source.geocodio > 0) parts.push(`${stats.by_source.geocodio} via API`);
  if (stats.unresolved > 0) parts.push(`${stats.unresolved} unresolved`);

  const ratio = stats.total > 0 ? (stats.geocoded / stats.total) * 100 : 0;

  return (
    <div className="flex items-center gap-1.5">
      {/* Map pin icon */}
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true" className="text-muted-foreground shrink-0">
        <path d="M6 1C4.067 1 2.5 2.567 2.5 4.5C2.5 7.25 6 11 6 11s3.5-3.75 3.5-6.5C9.5 2.567 7.933 1 6 1z" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
        <circle cx="6" cy="4.5" r="1.25" stroke="currentColor" strokeWidth="1.2" />
      </svg>
      <p className="text-xs text-muted-foreground">
        {stats.geocoded}/{stats.total}
        {parts.length > 0 ? ` · ${parts.join(", ")}` : ""}
      </p>
      <div className="max-w-[60px] h-1 rounded-full overflow-hidden bg-muted shrink-0">
        <div
          className="h-full bg-success rounded-full"
          style={{ width: `${ratio}%`, transition: "width 300ms ease" }}
        />
      </div>
    </div>
  );
}

type PreviewData = {
  columns: string[];
  rows: Record<string, string | null>[];
  total: number;
};

function TablePreview({ tableName }: { tableName: string }) {
  const [data, setData] = useState<PreviewData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    invoke<string>("preview_table", { tableName, limit: 50 })
      .then((raw) => {
        setData(JSON.parse(raw) as PreviewData);
      })
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [tableName]);

  if (loading) return <Spinner size="sm" />;
  if (error) return <p className="text-xs text-destructive">{error}</p>;
  if (!data || data.rows.length === 0) return <p className="text-xs text-muted-foreground">No rows</p>;

  return (
    <div className="overflow-x-auto overflow-y-auto max-h-60">
      <table className="w-full border-collapse text-[11px]">
        <thead>
          <tr>
            {data.columns.map((col) => (
              <th key={col} className="px-1.5 py-1 border-b border-border text-left whitespace-nowrap font-semibold sticky top-0 bg-card">
                {col}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.rows.map((row, i) => (
            <tr key={i}>
              {data.columns.map((col) => (
                <td key={col} className="px-1.5 py-0.5 border-b border-border/50 whitespace-nowrap max-w-[150px] overflow-hidden text-ellipsis">
                  {row[col] ?? <span className="text-muted-foreground">null</span>}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {data.total >= 50 && (
        <p className="text-xs text-muted-foreground mt-1">Showing first 50 rows</p>
      )}
    </div>
  );
}

type FileListProps = {
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  onSettingsClick?: () => void;
};

export function FileList({ collapsed = false, onToggleCollapse, onSettingsClick }: FileListProps) {
  const tables = useAppStore((s) => s.tables);
  const addTable = useAppStore((s) => s.addTable);
  const updateTable = useAppStore((s) => s.updateTable);
  const removeTable = useAppStore((s) => s.removeTable);
  const setTables = useAppStore((s) => s.setTables);
  const setTableGeoJson = useAppStore((s) => s.setTableGeoJson);
  const clearTableGeoJson = useAppStore((s) => s.clearTableGeoJson);
  const apiConfig = useAppStore((s) => s.apiConfig);
  const selectedTablesForChat = useAppStore((s) => s.selectedTablesForChat);
  const toggleTableForChat = useAppStore((s) => s.toggleTableForChat);
  const logPath = useAppStore((s) => s.logPath);
  const domainConfig = useAppStore((s) => s.domainConfig);
  const geocodeColRef = useRef<Record<string, string>>({});
  const [previewTable, setPreviewTable] = useState<string | null>(null);

  // Load existing tables on mount
  useEffect(() => {
    if (!isTauri()) return;

    invoke<string>("list_tables").then((raw) => {
      try {
        const parsed = JSON.parse(raw) as { tables: Array<{ name: string }> };
        const existing = parsed.tables.map((t) => ({
          name: t.name,
          status: "done" as const,
          addressColumns: [],
        }));
        setTables(existing);
      } catch { /* ignore */ }
    }).catch(() => { /* ignore */ });
  }, [setTables]);

  // Listen for pipeline progress events
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | undefined;
    const attach = async () => {
      unlisten = await listen<{ table_name: string; stage: string; message: string; percent: number }>(
        "ingest-progress",
        (event) => {
          const { table_name: tableName, stage, message, percent } = event.payload;

          // Get current table state to guard against late-arriving events
          const currentTable = useAppStore.getState().tables.find((t) => t.name === tableName);
          if (!currentTable) return;
          if (currentTable.status === "done" || currentTable.status === "error") return;

          // Map stage string to TableInfo status
          const stageToStatus: Record<string, TableInfo["status"]> = {
            started: "ingesting",
            reading: "ingesting",
            writing: "ingesting",
            cleaning: "cleaning",
            detecting: "detecting",
            geocoding: "geocoding",
          };
          const mappedStatus = stageToStatus[stage];

          updateTable(tableName, {
            ...(mappedStatus ? { status: mappedStatus } : {}),
            progressMessage: message,
            progressPercent: percent,
          });
        }
      );
    };
    void attach();
    return () => unlisten?.();
  }, [updateTable]);

  async function handleAddFiles() {
    if (!isTauri()) return;

    const selected = await open({
      multiple: true,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });

    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];

    // Build the list of files to process and add them all to the UI immediately
    const filesToProcess = paths.map((csvPath) => ({
      csvPath,
      tableName: sanitizeTableName(csvPath),
    }));

    for (const { tableName } of filesToProcess) {
      addTable({
        name: tableName,
        status: "ingesting",
        progressMessage: "Queued...",
        progressPercent: 0,
        addressColumns: [],
      });
    }

    // Process files sequentially (DuckDB file-level locking requires this)
    for (const { csvPath, tableName } of filesToProcess) {
      updateTable(tableName, {
        progressMessage: "Starting pipeline...",
      });

      try {
        const raw = await invoke<string>("ingest_file_pipeline", {
          csvPath,
          tableName,
        });

        const result = JSON.parse(raw) as {
          status: "ready" | "done";
          table: string;
          row_count: number;
          clean_summary: string;
          address_columns: string[];
        };

        // "ready" means address columns were detected — wait for user to confirm geocoding
        // "done" means no address columns were found — pipeline is complete
        updateTable(tableName, {
          status: result.status,
          rowCount: result.row_count,
          cleanSummary: result.clean_summary,
          addressColumns: result.address_columns,
          progressMessage: undefined,
          progressPercent: 100,
        });
      } catch (err) {
        updateTable(tableName, {
          status: "error",
          error: String(err),
          progressMessage: undefined,
        });
      }
    }
  }

  async function handleGeocode(table: TableInfo) {
    const col = geocodeColRef.current[table.name] ?? table.addressColumns[0];
    if (!col) return;

    updateTable(table.name, {
      status: "geocoding",
      progressMessage: "Geocoding...",
      geocodeColumn: col,
    });

    try {
      const raw = await invoke<string>("geocode_table_column", {
        tableName: table.name,
        addressCol: col,
      });
      const geocodeResult = JSON.parse(raw) as {
        status: string;
        geocoded_count: number;
        total_addresses: number;
        by_source?: { cache: number; overture_fuzzy: number; geocodio: number };
        unresolved?: number;
      };
      const geocodeStats: GeocodeStats | undefined = geocodeResult.by_source
        ? {
            total: geocodeResult.total_addresses,
            geocoded: geocodeResult.geocoded_count,
            by_source: geocodeResult.by_source,
            unresolved: geocodeResult.unresolved ?? 0,
          }
        : undefined;
      updateTable(table.name, {
        status: "done",
        progressMessage: undefined,
        progressPercent: 100,
        geocodeColumn: col,
        geocodeStats,
      });
      void loadTableGeoJson(table.name, setTableGeoJson);
    } catch (err) {
      updateTable(table.name, {
        status: "error",
        error: String(err),
        progressMessage: undefined,
      });
    }
  }

  async function handleExportCsv(table: TableInfo) {
    if (!isTauri()) return;
    try {
      const filePath = await save({
        defaultPath: `${table.name}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (filePath) {
        await invoke("export_table_csv", { tableName: table.name, filePath });
      }
    } catch { /* ignore */ }
  }

  async function handleDelete(table: TableInfo) {
    try {
      await invoke("drop_table", { tableName: table.name });
    } catch { /* ignore */ }
    clearTableGeoJson(table.name);
    removeTable(table.name);
  }

  // Collapsed strip: shows only toggle, add button, and table count badge
  if (collapsed) {
    return (
      <div className="h-full flex flex-col items-center py-3 gap-3 overflow-hidden">
        <button
          onClick={onToggleCollapse}
          title="Expand panel"
          className="w-7 h-7 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors shrink-0"
        >
          {/* Left-pointing chevron: expand means revealing the panel to the left */}
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M9 11L5 7l4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>

        {isTauri() && (
          <button
            onClick={() => void handleAddFiles()}
            title="Add file"
            className="w-7 h-7 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors shrink-0 text-base leading-none"
          >
            +
          </button>
        )}

        {tables.length > 0 && (
          <span className="text-[10px] font-semibold bg-primary/20 text-primary rounded-full w-5 h-5 flex items-center justify-center shrink-0">
            {tables.length}
          </span>
        )}
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-3">
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-1.5">
          <button
            onClick={onToggleCollapse}
            title="Collapse panel"
            className="w-6 h-6 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors shrink-0"
          >
            {/* Right-pointing chevron: collapse hides the panel to the right */}
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M5 3l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
          <h2 className="text-sm font-semibold">Tables</h2>
        </div>
        <div className="flex items-center gap-1.5">
          {onSettingsClick && (
            <button
              onClick={onSettingsClick}
              title="Settings"
              className="w-6 h-6 flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            >
              {/* Gear icon */}
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                <path d="M5.7 1.3h2.6l.3 1.5.9.4 1.3-.8 1.8 1.8-.8 1.3.4.9 1.5.3v2.6l-1.5.3-.4.9.8 1.3-1.8 1.8-1.3-.8-.9.4-.3 1.5H5.7l-.3-1.5-.9-.4-1.3.8-1.8-1.8.8-1.3-.4-.9-1.5-.3V5.7l1.5-.3.4-.9-.8-1.3 1.8-1.8 1.3.8.9-.4.3-1.5z" stroke="currentColor" strokeWidth="1.1" strokeLinejoin="round" />
                <circle cx="7" cy="7" r="1.8" stroke="currentColor" strokeWidth="1.1" />
              </svg>
            </button>
          )}
          {isTauri() && (
            <Button size="sm" onClick={() => void handleAddFiles()}>
              + Add file
            </Button>
          )}
        </div>
      </div>

      {apiConfig !== null && !apiConfig.gemini && (
        <div className="rounded-lg border border-warning/30 bg-warning/10 p-2 mb-2">
          <p className="text-xs text-warning">
            AI cleaning will be skipped — set{" "}
            <strong>SPATIA_GEMINI_API_KEY</strong> to enable it.
          </p>
        </div>
      )}

      {tables.length === 0 && (
        <div
          className="flex flex-col items-center gap-3 py-8 px-3 rounded-xl"
          style={{
            border: "2px dashed rgba(139, 92, 246, 0.3)",
            background: "rgba(139, 92, 246, 0.04)",
          }}
        >
          <div className="flex flex-col items-center gap-1">
            <p className="text-sm font-semibold text-center">{domainConfig.ui_config.empty_state_title}</p>
            <p className="text-xs text-muted-foreground text-center">
              {domainConfig.ui_config.empty_state_description}
            </p>
          </div>
          <p className="text-xs text-muted-foreground text-center leading-relaxed">
            {domainConfig.ui_config.upload_instruction}
          </p>
          {isTauri() && (
            <Button
              size="lg"
              onClick={() => void handleAddFiles()}
              style={{ minWidth: "160px" }}
            >
              + Upload a CSV file
            </Button>
          )}
        </div>
      )}

      <div className="flex flex-col gap-2">
        {tables.map((table) => {
          const isSelected = selectedTablesForChat.has(table.name);
          const showPreview = previewTable === table.name;
          return (
          <Card
            key={table.name}
            className="table-card p-2.5"
            style={
              table.status === "done"
                ? { borderLeft: `3px solid ${domainConfig.ui_config.primary_color}` }
                : undefined
            }
          >
            <div className="flex flex-col gap-2">
              {/* Header row: [chat-toggle] [name + row count] [badge] */}
              <div className="flex items-center gap-2">
                {(table.status === "done" || table.status === "ready") && (
                  <button
                    onClick={() => toggleTableForChat(table.name)}
                    title={isSelected ? "Remove from chat context" : "Add to chat context"}
                    className="shrink-0 text-muted-foreground hover:text-primary transition-colors"
                  >
                    {isSelected ? (
                      /* Filled chat bubble */
                      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                        <path d="M2 2.5A1.5 1.5 0 013.5 1h7A1.5 1.5 0 0112 2.5v6A1.5 1.5 0 0110.5 10H5.5L3 12.5V10H3.5A1.5 1.5 0 012 8.5v-6z" fill="currentColor" />
                      </svg>
                    ) : (
                      /* Outline chat bubble */
                      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                        <path d="M2 2.5A1.5 1.5 0 013.5 1h7A1.5 1.5 0 0112 2.5v6A1.5 1.5 0 0110.5 10H5.5L3 12.5V10H3.5A1.5 1.5 0 012 8.5v-6z" stroke="currentColor" strokeWidth="1.2" />
                      </svg>
                    )}
                  </button>
                )}
                <div className="flex flex-col flex-1 min-w-0">
                  <div className="flex items-center gap-1.5">
                    <p className="text-sm font-medium break-all">{table.name}</p>
                    {table.rowCount != null && (
                      <span className="text-xs text-muted-foreground shrink-0">
                        ({table.rowCount.toLocaleString()})
                      </span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  {isActive(table.status) && <Spinner size="sm" />}
                  <Badge variant={statusVariant(table.status)}>
                    {statusIcon(table.status) && (
                      <span className="mr-1" aria-hidden="true">
                        {statusIcon(table.status)}
                      </span>
                    )}
                    {table.status}
                  </Badge>
                </div>
              </div>

              {isActive(table.status) && table.progressMessage && (
                <p className="text-xs text-muted-foreground">{table.progressMessage}</p>
              )}

              {isActive(table.status) && table.progressPercent != null && (
                <div className="w-full rounded-full overflow-hidden bg-muted" style={{ height: "2px" }}>
                  <div
                    className="h-full bg-primary rounded-full"
                    style={{
                      width: `${table.progressPercent}%`,
                      transition: "width 300ms ease",
                    }}
                  />
                </div>
              )}

              {table.cleanSummary && !isActive(table.status) && (
                <p className="text-xs text-muted-foreground">Clean: {table.cleanSummary}</p>
              )}

              {table.error && (
                <div>
                  <p className="text-xs text-destructive">{table.error}</p>
                  {logPath && (
                    <p className="text-xs text-muted-foreground mt-1">
                      For details, see log:{" "}
                      <code className="font-mono text-[10px]">{logPath}</code>
                    </p>
                  )}
                </div>
              )}

              {table.geocodeWarning && table.status === "done" && (
                <p className="text-xs text-warning">Geocode skipped: {table.geocodeWarning}</p>
              )}

              {/* Inline geocode confirmation — shown when address columns were detected */}
              {table.status === "ready" && table.addressColumns.length > 0 && (
                <div className="rounded-md border border-warning/40 bg-warning/10 p-2 flex flex-col gap-2">
                  <div className="flex flex-col gap-1">
                    <p className="text-xs font-medium">
                      Address column detected:{" "}
                      {table.addressColumns.length === 1 ? (
                        <span className="font-semibold">{table.addressColumns[0]}</span>
                      ) : (
                        <Select
                          value={geocodeColRef.current[table.name] ?? table.addressColumns[0]}
                          onChange={(e) => { geocodeColRef.current[table.name] = e.target.value; }}
                        >
                          {table.addressColumns.map((col) => (
                            <option key={col} value={col}>{col}</option>
                          ))}
                        </Select>
                      )}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      Geocode these addresses to plot them on the map?
                    </p>
                    {apiConfig !== null && !apiConfig.geocodio && (
                      <p className="text-xs text-muted-foreground italic">
                        Only local address matching available — set SPATIA_GEOCODIO_API_KEY for API fallback.
                      </p>
                    )}
                  </div>
                  <div className="flex gap-2">
                    <Button size="sm" onClick={() => void handleGeocode(table)}>
                      Geocode
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => updateTable(table.name, { status: "done" })}
                    >
                      Skip
                    </Button>
                  </div>
                </div>
              )}

              {/* Address column info and re-geocode option for "done" tables */}
              {table.status === "done" && table.addressColumns.length > 0 && (
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-xs">Address col:</span>
                  {table.addressColumns.length === 1 ? (
                    <span className="text-xs font-medium">{table.addressColumns[0]}</span>
                  ) : (
                    <Select
                      value={geocodeColRef.current[table.name] ?? table.addressColumns[0]}
                      onChange={(e) => { geocodeColRef.current[table.name] = e.target.value; }}
                    >
                      {table.addressColumns.map((col) => (
                        <option key={col} value={col}>{col}</option>
                      ))}
                    </Select>
                  )}
                  {!table.geocodeColumn && (
                    <Button size="sm" variant="secondary" onClick={() => void handleGeocode(table)}>
                      Geocode
                    </Button>
                  )}
                  {table.geocodeColumn && (
                    <span className="text-xs text-success">Geocoded</span>
                  )}
                </div>
              )}

              {table.status === "done" && table.geocodeColumn && table.geocodeStats && (
                <GeocodeStatsSummary stats={table.geocodeStats} />
              )}

              {/* Compact action footer */}
              {(table.status === "ready" || table.status === "done") && (
                <div className="flex items-center justify-between border-t border-border/50 pt-1.5">
                  <button
                    onClick={() => setPreviewTable(showPreview ? null : table.name)}
                    className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  >
                    {/* Rotating chevron */}
                    <svg
                      width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true"
                      className="transition-transform duration-150"
                      style={{ transform: showPreview ? "rotate(90deg)" : "rotate(0deg)" }}
                    >
                      <path d="M4.5 2.5l3.5 3.5-3.5 3.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                    Preview
                  </button>
                  <div className="flex items-center gap-1.5">
                    {isTauri() && (
                      <button
                        onClick={() => void handleExportCsv(table)}
                        title="Export as CSV"
                        className="text-muted-foreground hover:text-foreground transition-colors"
                      >
                        {/* Download icon */}
                        <svg width="13" height="13" viewBox="0 0 13 13" fill="none" aria-hidden="true">
                          <path d="M6.5 1.5v7M6.5 8.5l-2.5-2.5M6.5 8.5l2.5-2.5M2 10.5h9" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                      </button>
                    )}
                    <button
                      onClick={() => void handleDelete(table)}
                      disabled={isActive(table.status)}
                      title="Delete table"
                      className="text-muted-foreground hover:text-destructive transition-colors disabled:opacity-30"
                    >
                      {/* Trash icon */}
                      <svg width="13" height="13" viewBox="0 0 13 13" fill="none" aria-hidden="true">
                        <path d="M2 3.5h9M4.5 3.5V2.5a1 1 0 011-1h2a1 1 0 011 1v1M5.5 6v3M7.5 6v3M3.5 3.5l.5 7a1 1 0 001 1h3a1 1 0 001-1l.5-7" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round" />
                      </svg>
                    </button>
                  </div>
                </div>
              )}

              {showPreview && (table.status === "ready" || table.status === "done") && (
                <TablePreview tableName={table.name} />
              )}
            </div>
          </Card>
          );
        })}
      </div>
    </div>
  );
}
