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

const BAR_FILL = "#7c3aed";
const BAR_FILL_HOVER = "#8b5cf6";

function isNumeric(value: string | null): boolean {
  if (value === null || value === "") return false;
  return !isNaN(Number(value)) && !isNaN(parseFloat(value));
}

/** Sturges' rule: ceil(log2(n) + 1) */
function sturgesBinCount(n: number): number {
  if (n <= 1) return 1;
  return Math.ceil(Math.log2(n) + 1);
}

type BinRow = {
  range: string;
  count: number;
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
      <p className="font-semibold text-foreground">
        {payload[0].value} rows
      </p>
    </div>
  );
}

type Props = {
  data: ResultRows;
};

export function HistogramWidget({ data }: Props) {
  const { columns, rows } = data;

  if (columns.length === 0 || rows.length === 0) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No data to display
      </p>
    );
  }

  // Find first numeric column
  let valueColIdx = -1;
  for (let ci = 0; ci < columns.length; ci++) {
    const firstNonNull = rows.find((r) => r[ci] !== null)?.[ci] ?? null;
    if (isNumeric(firstNonNull)) {
      valueColIdx = ci;
      break;
    }
  }

  if (valueColIdx === -1) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No numeric column found
      </p>
    );
  }

  // Extract numeric values, skip nulls
  const values: number[] = rows
    .map((r) => r[valueColIdx])
    .filter((v) => v !== null && isNumeric(v))
    .map((v) => parseFloat(v as string))
    .filter((v) => !isNaN(v));

  if (values.length === 0) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No numeric data to display
      </p>
    );
  }

  const min = Math.min(...values);
  const max = Math.max(...values);
  const binCount = sturgesBinCount(values.length);

  // Edge case: all values identical → single bin
  let bins: BinRow[];
  if (min === max) {
    bins = [{ range: String(min), count: values.length }];
  } else {
    const binWidth = (max - min) / binCount;

    // Initialize bins
    bins = Array.from({ length: binCount }, (_, i) => {
      const lo = min + i * binWidth;
      const hi = lo + binWidth;
      // Format numbers compactly
      const fmt = (n: number) =>
        Number.isInteger(n) ? String(n) : n.toFixed(2);
      return { range: `${fmt(lo)}-${fmt(hi)}`, count: 0 };
    });

    // Assign values to bins
    for (const v of values) {
      // Clamp max value to last bin
      const idx = Math.min(
        Math.floor((v - min) / binWidth),
        binCount - 1
      );
      bins[idx].count += 1;
    }
  }

  const manyBins = bins.length > 8;
  const bottomMargin = manyBins ? 60 : 20;

  return (
    <div className="w-full h-full flex flex-col">
      <div style={{ width: "100%", height: "250px" }}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={bins}
            margin={{ top: 8, right: 12, left: 0, bottom: bottomMargin }}
          >
            <CartesianGrid
              strokeDasharray="3 3"
              stroke="rgba(148, 163, 184, 0.15)"
              vertical={false}
            />
            <XAxis
              dataKey="range"
              tick={{ fill: "#94a3b8", fontSize: 10 }}
              angle={manyBins ? -45 : 0}
              textAnchor={manyBins ? "end" : "middle"}
              interval={0}
              axisLine={{ stroke: "rgba(148, 163, 184, 0.2)" }}
              tickLine={false}
            />
            <YAxis
              tick={{ fill: "#94a3b8", fontSize: 11 }}
              axisLine={false}
              tickLine={false}
              width={48}
              label={{
                value: "Count",
                angle: -90,
                position: "insideLeft",
                offset: 10,
                style: { fill: "#94a3b8", fontSize: 10 },
              }}
            />
            <Tooltip
              content={<CustomTooltip />}
              cursor={{ fill: "rgba(139, 92, 246, 0.08)" }}
            />
            <Bar dataKey="count" radius={[3, 3, 0, 0]} maxBarSize={56}>
              {bins.map((_bin, index) => (
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
