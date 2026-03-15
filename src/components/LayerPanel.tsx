import { useState } from "react";
import { useAppStore } from "../lib/appStore";
import type { RiskLayerInfo } from "../lib/appStore";

export function LayerPanel() {
  const [open, setOpen] = useState(false);
  const tableGeoJson = useAppStore((s) => s.tableGeoJson);
  const analysisGeoJson = useAppStore((s) => s.analysisGeoJson);
  const riskLayers = useAppStore((s) => s.riskLayers);
  const riskLayerGeoJson = useAppStore((s) => s.riskLayerGeoJson);
  const layerVisibility = useAppStore((s) => s.layerVisibility);
  const layerOpacity = useAppStore((s) => s.layerOpacity);
  const toggleLayerVisibility = useAppStore((s) => s.toggleLayerVisibility);
  const setLayerOpacity = useAppStore((s) => s.setLayerOpacity);

  const analysisFeatures = (analysisGeoJson as { features?: unknown[] })?.features ?? [];
  const hasAnalysis = analysisFeatures.length > 0;
  const tableNames = Object.keys(tableGeoJson);
  const riskNames = Object.keys(riskLayerGeoJson);

  const hasLayers = hasAnalysis || tableNames.length > 0 || riskNames.length > 0;

  if (!hasLayers) return null;

  const isVisible = (id: string) => layerVisibility[id] !== false;
  const getOpacity = (id: string) => layerOpacity[id] ?? 1;

  return (
    <>
      {/* Toggle button */}
      <button
        onClick={() => setOpen(!open)}
        title="Toggle layer panel"
        style={{
          position: "absolute",
          top: 10,
          left: 10,
          zIndex: 5,
          background: open ? "rgba(124, 58, 237, 0.3)" : "rgba(15, 15, 20, 0.85)",
          backdropFilter: "blur(8px)",
          border: "1px solid rgba(148, 163, 184, 0.15)",
          borderRadius: 6,
          padding: "5px 10px",
          color: "rgba(255,255,255,0.7)",
          fontSize: 11,
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          gap: 4,
        }}
      >
        {/* Layers icon */}
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <path d="M7 1L1 4.5L7 8L13 4.5L7 1Z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
          <path d="M1 7L7 10.5L13 7" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
          <path d="M1 9.5L7 13L13 9.5" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
        </svg>
        Layers
      </button>

      {/* Panel */}
      {open && (
        <div
          style={{
            position: "absolute",
            top: 44,
            left: 10,
            zIndex: 5,
            background: "rgba(15, 15, 20, 0.92)",
            backdropFilter: "blur(12px)",
            border: "1px solid rgba(148, 163, 184, 0.15)",
            borderRadius: 8,
            padding: "8px 0",
            minWidth: 220,
            maxHeight: 400,
            overflowY: "auto",
            fontSize: 12,
            color: "rgba(255,255,255,0.85)",
          }}
        >
          {/* Analysis result layer */}
          {hasAnalysis && (
            <LayerRow
              id="analysis"
              label="Analysis Result"
              color="#7c3aed"
              visible={isVisible("analysis")}
              opacity={getOpacity("analysis")}
              onToggle={() => toggleLayerVisibility("analysis")}
              onOpacity={(v) => setLayerOpacity("analysis", v)}
            />
          )}

          {/* Risk layers */}
          {riskNames.length > 0 && (
            <>
              <div
                style={{
                  padding: "4px 12px 2px",
                  fontSize: 10,
                  color: "rgba(255,255,255,0.4)",
                  textTransform: "uppercase",
                  letterSpacing: 1,
                }}
              >
                Risk Overlays
              </div>
              {riskNames.map((name) => {
                const info = riskLayers.find((l: RiskLayerInfo) => l.name === name);
                return (
                  <LayerRow
                    key={name}
                    id={`risk-${name}`}
                    label={info?.display_name ?? name}
                    color={getRiskColor(info?.layer_type ?? "custom")}
                    visible={isVisible(`risk-${name}`)}
                    opacity={getOpacity(`risk-${name}`)}
                    onToggle={() => toggleLayerVisibility(`risk-${name}`)}
                    onOpacity={(v) => setLayerOpacity(`risk-${name}`, v)}
                  />
                );
              })}
            </>
          )}

          {/* User data layers */}
          {tableNames.length > 0 && (
            <>
              <div
                style={{
                  padding: "4px 12px 2px",
                  fontSize: 10,
                  color: "rgba(255,255,255,0.4)",
                  textTransform: "uppercase",
                  letterSpacing: 1,
                }}
              >
                Data Layers
              </div>
              {tableNames.map((name) => (
                <LayerRow
                  key={name}
                  id={`table-${name}`}
                  label={name}
                  color="#2563eb"
                  visible={isVisible(`table-${name}`)}
                  opacity={getOpacity(`table-${name}`)}
                  onToggle={() => toggleLayerVisibility(`table-${name}`)}
                  onOpacity={(v) => setLayerOpacity(`table-${name}`, v)}
                />
              ))}
            </>
          )}
        </div>
      )}
    </>
  );
}

function getRiskColor(layerType: string): string {
  switch (layerType) {
    case "flood":
      return "#3b82f6"; // blue
    case "wildfire":
      return "#ef4444"; // red
    case "wind":
      return "#06b6d4"; // cyan
    default:
      return "#f59e0b"; // amber
  }
}

function LayerRow({
  id: _id,
  label,
  color,
  visible,
  opacity,
  onToggle,
  onOpacity,
}: {
  id: string;
  label: string;
  color: string;
  visible: boolean;
  opacity: number;
  onToggle: () => void;
  onOpacity: (v: number) => void;
}) {
  return (
    <div
      style={{
        padding: "4px 12px",
        display: "flex",
        alignItems: "center",
        gap: 8,
        opacity: visible ? 1 : 0.4,
      }}
    >
      {/* Visibility toggle */}
      <button
        onClick={onToggle}
        title={visible ? "Hide layer" : "Show layer"}
        style={{
          background: "none",
          border: "none",
          cursor: "pointer",
          padding: 0,
          color: visible ? "rgba(255,255,255,0.7)" : "rgba(255,255,255,0.3)",
          fontSize: 14,
          lineHeight: 1,
        }}
      >
        {visible ? (
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path
              d="M1 7s2.5-4 6-4 6 4 6 4-2.5 4-6 4-6-4-6-4z"
              stroke="currentColor"
              strokeWidth="1.2"
            />
            <circle cx="7" cy="7" r="2" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        ) : (
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path
              d="M1 7s2.5-4 6-4 6 4 6 4-2.5 4-6 4-6-4-6-4z"
              stroke="currentColor"
              strokeWidth="1.2"
            />
            <line x1="2" y1="2" x2="12" y2="12" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        )}
      </button>

      {/* Color dot */}
      <span
        style={{
          width: 8,
          height: 8,
          borderRadius: "50%",
          background: color,
          flexShrink: 0,
        }}
      />

      {/* Label */}
      <span
        style={{
          flex: 1,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {label}
      </span>

      {/* Opacity slider */}
      <input
        type="range"
        min={0}
        max={1}
        step={0.1}
        value={opacity}
        onChange={(e) => onOpacity(parseFloat(e.target.value))}
        title={`Opacity: ${Math.round(opacity * 100)}%`}
        style={{
          width: 50,
          height: 3,
          accentColor: color,
          cursor: "pointer",
        }}
      />
    </div>
  );
}
