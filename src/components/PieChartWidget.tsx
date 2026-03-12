import {
  PieChart,
  Pie,
  Cell,
  Tooltip,
  Legend,
  ResponsiveContainer,
} from "recharts";
import type { PieLabelRenderProps } from "recharts";
import type { ResultRows } from "../lib/appStore";

const MAX_SLICES = 8;

// 8 distinct colors that work well on dark backgrounds
const COLORS = [
  "#7c3aed", // purple
  "#2563eb", // blue
  "#0891b2", // cyan
  "#059669", // teal/green
  "#d97706", // amber
  "#dc2626", // red
  "#db2777", // pink
  "#7c3aed", // (wraps — shouldn't reach given max 8)
];

// Unique palette so "Other" slice is clearly distinguishable
const OTHER_COLOR = "#475569"; // slate-600

function isNumeric(value: string | null): boolean {
  if (value === null || value === "") return false;
  return !isNaN(Number(value)) && !isNaN(parseFloat(value));
}

type PieRow = {
  name: string;
  value: number;
};

type CustomTooltipProps = {
  active?: boolean;
  payload?: Array<{ name: string; value: number; payload: PieRow }>;
};

function CustomTooltip({ active, payload }: CustomTooltipProps) {
  if (!active || !payload?.length) return null;
  const entry = payload[0];
  return (
    <div className="rounded border border-border bg-card px-3 py-2 text-xs shadow-lg">
      <p className="text-muted-foreground mb-1">{entry.name}</p>
      <p className="font-semibold text-foreground">{entry.value}</p>
    </div>
  );
}

type Props = {
  data: ResultRows;
};

export function PieChartWidget({ data }: Props) {
  const { columns, rows } = data;

  if (columns.length === 0 || rows.length === 0) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No data to display
      </p>
    );
  }

  // Detect label column (first non-numeric) and value column (first numeric)
  let labelColIdx = -1;
  let valueColIdx = -1;

  for (let ci = 0; ci < columns.length; ci++) {
    const firstNonNull = rows.find((r) => r[ci] !== null)?.[ci] ?? null;
    if (!isNumeric(firstNonNull)) {
      if (labelColIdx === -1) labelColIdx = ci;
    } else {
      if (valueColIdx === -1) valueColIdx = ci;
    }
    if (labelColIdx !== -1 && valueColIdx !== -1) break;
  }

  if (valueColIdx === -1) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No numeric column found
      </p>
    );
  }

  const usesIndexLabel = labelColIdx === -1;

  // Build pie rows, filtering nulls in value column
  let pieRows: PieRow[] = rows
    .filter((r) => r[valueColIdx] !== null)
    .map((r, i) => ({
      name: usesIndexLabel
        ? String(i + 1)
        : (r[labelColIdx] ?? "(null)"),
      value: parseFloat(r[valueColIdx] as string),
    }))
    .filter((r) => !isNaN(r.value));

  if (pieRows.length === 0) {
    return (
      <p className="text-xs text-muted-foreground text-center py-8">
        No numeric data to display
      </p>
    );
  }

  // Collapse rows beyond MAX_SLICES into an "Other" slice
  if (pieRows.length > MAX_SLICES) {
    const top = pieRows.slice(0, MAX_SLICES - 1);
    const rest = pieRows.slice(MAX_SLICES - 1);
    const otherValue = rest.reduce((sum, r) => sum + r.value, 0);
    pieRows = [...top, { name: "Other", value: otherValue }];
  }

  const total = pieRows.reduce((sum, r) => sum + r.value, 0);

  // Custom label renderer: percentage inside each slice
  const renderLabel = (props: PieLabelRenderProps) => {
    const {
      cx,
      cy,
      midAngle,
      innerRadius,
      outerRadius,
      index,
    } = props;

    // All of these can be undefined per the Recharts type — guard defensively
    if (
      cx == null ||
      cy == null ||
      midAngle == null ||
      innerRadius == null ||
      outerRadius == null ||
      index == null
    ) {
      return null;
    }

    const cxNum = Number(cx);
    const cyNum = Number(cy);
    const innerNum = Number(innerRadius);
    const outerNum = Number(outerRadius);

    const RADIAN = Math.PI / 180;
    const radius = innerNum + (outerNum - innerNum) * 0.55;
    const x = cxNum + radius * Math.cos(-midAngle * RADIAN);
    const y = cyNum + radius * Math.sin(-midAngle * RADIAN);

    const pct =
      total > 0
        ? ((pieRows[index].value / total) * 100).toFixed(1)
        : "0.0";

    // Skip tiny slices to avoid label clutter
    if (parseFloat(pct) < 5) return null;

    return (
      <text
        x={x}
        y={y}
        fill="#f8fafc"
        textAnchor="middle"
        dominantBaseline="central"
        fontSize={10}
        fontWeight={500}
      >
        {pct}%
      </text>
    );
  };

  return (
    <div className="w-full h-full flex flex-col">
      <div style={{ width: "100%", height: "250px" }}>
        <ResponsiveContainer width="100%" height="100%">
          <PieChart margin={{ top: 8, right: 8, left: 8, bottom: 8 }}>
            <Pie
              data={pieRows}
              dataKey="value"
              nameKey="name"
              cx="50%"
              cy="45%"
              outerRadius="60%"
              labelLine={false}
              label={renderLabel}
            >
              {pieRows.map((entry, index) => (
                <Cell
                  key={`cell-${index}`}
                  fill={
                    entry.name === "Other"
                      ? OTHER_COLOR
                      : COLORS[index % (COLORS.length - 1)]
                  }
                />
              ))}
            </Pie>
            <Tooltip content={<CustomTooltip />} />
            <Legend
              iconSize={10}
              wrapperStyle={{ fontSize: "11px", color: "#94a3b8" }}
            />
          </PieChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
