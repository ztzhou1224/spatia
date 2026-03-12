import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Cell,
} from "recharts";
import type { ResultRows } from "../lib/appStore";

// Primary color from App.css design tokens
const BAR_FILL = "#7c3aed";
const BAR_FILL_HOVER = "#8b5cf6";
const MAX_BARS = 20;
const LABEL_MAX_LEN = 15;

function truncateLabel(label: string): string {
  if (label.length <= LABEL_MAX_LEN) return label;
  return label.slice(0, LABEL_MAX_LEN - 1) + "…";
}

function isNumeric(value: string | null): boolean {
  if (value === null || value === "") return false;
  return !isNaN(Number(value)) && !isNaN(parseFloat(value));
}

type ChartRow = {
  label: string;
  value: number;
  rawLabel: string;
};

type Props = {
  data: ResultRows;
};

type CustomTooltipProps = {
  active?: boolean;
  payload?: Array<{ value: number }>;
  label?: string;
};

function CustomTooltip({ active, payload, label }: CustomTooltipProps) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded border border-border bg-card px-3 py-2 text-xs shadow-lg">
      <p className="text-muted-foreground mb-1">{label}</p>
      <p className="font-semibold text-foreground">{payload[0].value}</p>
    </div>
  );
}

export function ChartWidget({ data }: Props) {
  const { columns, rows } = data;

  // Need at least one column and one row
  if (columns.length === 0 || rows.length === 0) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No data to display
      </p>
    );
  }

  // Detect category column (first non-numeric) and value column (first numeric)
  // Detect based on the first non-null value in each column
  let categoryColIdx = -1;
  let valueColIdx = -1;

  for (let ci = 0; ci < columns.length; ci++) {
    const firstNonNull = rows.find((r) => r[ci] !== null)?.[ci] ?? null;
    if (!isNumeric(firstNonNull)) {
      if (categoryColIdx === -1) categoryColIdx = ci;
    } else {
      if (valueColIdx === -1) valueColIdx = ci;
    }
    if (categoryColIdx !== -1 && valueColIdx !== -1) break;
  }

  // If no numeric column found, fall back gracefully
  if (valueColIdx === -1) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No numeric column found — showing table instead
      </p>
    );
  }

  // If no category column found, use row index as label
  const usesIndexLabel = categoryColIdx === -1;

  // Build chart rows, filtering out null values
  let chartRows: ChartRow[] = rows
    .filter((r) => r[valueColIdx] !== null)
    .map((r, i) => {
      const rawLabel = usesIndexLabel
        ? String(i + 1)
        : (r[categoryColIdx] ?? "(null)");
      return {
        rawLabel,
        label: truncateLabel(rawLabel),
        value: parseFloat(r[valueColIdx] as string),
      };
    });

  let truncated = false;
  if (chartRows.length > MAX_BARS) {
    // Sort by value desc and take top 20
    chartRows = chartRows
      .sort((a, b) => b.value - a.value)
      .slice(0, MAX_BARS);
    truncated = true;
  }

  const manyBars = chartRows.length > 8;
  // Estimate bottom margin for angled labels
  const bottomMargin = manyBars ? 60 : 20;

  return (
    <div className="w-full h-full flex flex-col">
      {truncated && (
        <p className="text-[10px] text-muted-foreground mb-1 text-right pr-1">
          Showing top {MAX_BARS}
        </p>
      )}
      <div style={{ width: "100%", height: "250px" }}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={chartRows}
            margin={{ top: 8, right: 12, left: 0, bottom: bottomMargin }}
          >
            <CartesianGrid
              strokeDasharray="3 3"
              stroke="rgba(148, 163, 184, 0.15)"
              vertical={false}
            />
            <XAxis
              dataKey="label"
              tick={{
                fill: "#94a3b8",
                fontSize: 11,
              }}
              angle={manyBars ? -45 : 0}
              textAnchor={manyBars ? "end" : "middle"}
              interval={0}
              axisLine={{ stroke: "rgba(148, 163, 184, 0.2)" }}
              tickLine={false}
            />
            <YAxis
              tick={{ fill: "#94a3b8", fontSize: 11 }}
              axisLine={false}
              tickLine={false}
              width={48}
            />
            <Tooltip
              content={
                <CustomTooltip />
              }
              cursor={{ fill: "rgba(139, 92, 246, 0.08)" }}
            />
            <Bar dataKey="value" radius={[3, 3, 0, 0]} maxBarSize={56}>
              {chartRows.map((_row, index) => (
                <Cell
                  key={index}
                  fill={BAR_FILL}
                  style={{ transition: "fill 150ms" }}
                  onMouseEnter={(e) => {
                    (e.target as SVGElement).setAttribute(
                      "fill",
                      BAR_FILL_HOVER
                    );
                  }}
                  onMouseLeave={(e) => {
                    (e.target as SVGElement).setAttribute("fill", BAR_FILL);
                  }}
                />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
