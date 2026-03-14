import { useAppStore } from "../lib/appStore";
import type { ResultRows } from "../lib/appStore";
import { ChartWidget } from "./ChartWidget";
import { PieChartWidget } from "./PieChartWidget";
import { HistogramWidget } from "./HistogramWidget";

function WidgetTable({ data }: { data: ResultRows }) {
  const { columns, rows, truncated } = data;

  if (columns.length === 0) {
    return (
      <p className="text-xs text-muted-foreground p-3">No results</p>
    );
  }

  return (
    <div className="flex flex-col min-h-0 flex-1">
      <div className="overflow-auto flex-1">
        <table className="min-w-full border-collapse text-[11px] font-mono whitespace-nowrap">
          <thead className="sticky top-0">
            <tr>
              {columns.map((col) => (
                <th
                  key={col}
                  className="px-2 py-1.5 text-left bg-secondary border-b border-border font-semibold text-foreground"
                >
                  {col}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td
                  colSpan={columns.length}
                  className="px-2 py-3 text-center text-muted-foreground"
                >
                  No results
                </td>
              </tr>
            ) : (
              rows.map((row, ri) => (
                <tr
                  key={ri}
                  className={ri % 2 === 0 ? "" : "bg-secondary/40"}
                >
                  {row.map((cell, ci) => (
                    <td
                      key={ci}
                      className="px-2 py-0.5 border-b border-border/40 max-w-[220px] overflow-hidden text-ellipsis"
                      title={cell ?? "null"}
                    >
                      {cell === null ? (
                        <em className="text-muted-foreground not-italic opacity-60">
                          null
                        </em>
                      ) : (
                        cell
                      )}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {truncated && (
        <p className="text-[10px] text-muted-foreground px-3 py-1.5 border-t border-border/40 shrink-0">
          Showing first {rows.length} rows
        </p>
      )}
    </div>
  );
}

export function WidgetPanel() {
  const activeWidget = useAppStore((s) => s.activeWidget);
  const clearActiveWidget = useAppStore((s) => s.clearActiveWidget);

  if (!activeWidget) return null;

  const truncatedTitle =
    activeWidget.title.length > 60
      ? activeWidget.title.slice(0, 57) + "..."
      : activeWidget.title;

  return (
    <div
      className="absolute bottom-24 left-1/2 -translate-x-1/2 z-20 glass-panel rounded-xl border border-border overflow-hidden flex flex-col"
      style={{
        maxWidth: "700px",
        width: "calc(100vw - 400px)",
        maxHeight: "420px",
        boxShadow: "0 8px 32px rgba(0,0,0,0.5), 0 2px 8px rgba(0,0,0,0.3)",
      }}
    >
      {/* Title bar */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-border/60 shrink-0">
        <span className="text-sm font-semibold text-foreground truncate pr-3">
          {truncatedTitle}
        </span>
        <button
          onClick={clearActiveWidget}
          className="shrink-0 text-muted-foreground hover:text-foreground transition-colors leading-none w-4 h-4 flex items-center justify-center rounded"
          aria-label="Close widget"
          title="Close widget"
        >
          &#x2715;
        </button>
      </div>

      {/* Content */}
      {activeWidget.type === "table" && (
        <WidgetTable data={activeWidget.data} />
      )}
      {activeWidget.type === "bar_chart" && (
        <div className="flex-1 min-h-0 p-3" style={{ minHeight: "280px" }}>
          <ChartWidget data={activeWidget.data} />
        </div>
      )}
      {activeWidget.type === "pie_chart" && (
        <div className="flex-1 min-h-0 p-3" style={{ minHeight: "280px" }}>
          <PieChartWidget data={activeWidget.data} />
        </div>
      )}
      {activeWidget.type === "histogram" && (
        <div className="flex-1 min-h-0 p-3" style={{ minHeight: "280px" }}>
          <HistogramWidget data={activeWidget.data} />
        </div>
      )}
    </div>
  );
}
