import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { isTauri } from "../lib/tauri";
import { useAppStore } from "../lib/appStore";

type Props = {
  open: boolean;
  onClose: () => void;
};

// ---- Shared style constants ----

const PANEL_STYLE: React.CSSProperties = {
  position: "relative",
  width: 560,
  maxWidth: "92vw",
  maxHeight: "88vh",
  overflowY: "auto",
  background: "rgba(15, 15, 22, 0.97)",
  border: "1px solid rgba(148, 163, 184, 0.15)",
  borderRadius: 14,
  padding: 28,
  boxShadow: "0 24px 64px rgba(0,0,0,0.75), 0 4px 16px rgba(0,0,0,0.5)",
};

const STEP_BTN_BASE: React.CSSProperties = {
  padding: "8px 18px",
  fontSize: 12,
  fontWeight: 600,
  borderRadius: 7,
  cursor: "pointer",
  border: "none",
  transition: "background 150ms",
};

const PRIMARY_BTN: React.CSSProperties = {
  ...STEP_BTN_BASE,
  background: "rgba(124, 58, 237, 0.75)",
  color: "#fff",
};

const SECONDARY_BTN: React.CSSProperties = {
  ...STEP_BTN_BASE,
  background: "rgba(148, 163, 184, 0.12)",
  color: "rgba(255,255,255,0.7)",
};

const DISABLED_BTN: React.CSSProperties = {
  ...STEP_BTN_BASE,
  background: "rgba(124, 58, 237, 0.25)",
  color: "rgba(255,255,255,0.35)",
  cursor: "default",
};

const CARD_STYLE: React.CSSProperties = {
  background: "rgba(255,255,255,0.04)",
  border: "1px solid rgba(148,163,184,0.12)",
  borderRadius: 9,
  padding: "14px 16px",
};

const LABEL_STYLE: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 600,
  color: "rgba(255,255,255,0.45)",
  textTransform: "uppercase" as const,
  letterSpacing: "0.06em",
  marginBottom: 4,
};

const VALUE_STYLE: React.CSSProperties = {
  fontSize: 22,
  fontWeight: 700,
  color: "#fff",
};

const NOTE_STYLE: React.CSSProperties = {
  fontSize: 11,
  color: "rgba(255,255,255,0.45)",
  marginTop: 2,
};

// ---- Step indicator ----

const STEPS = [
  "Import Portfolio",
  "Review Geocoding",
  "Load Risk Layers",
  "View Assessment",
  "Export Results",
];

function StepIndicator({ currentStep }: { currentStep: number }) {
  return (
    <div style={{ display: "flex", alignItems: "center", marginBottom: 28 }}>
      {STEPS.map((label, idx) => {
        const isComplete = idx < currentStep;
        const isActive = idx === currentStep;
        return (
          <div key={idx} style={{ display: "flex", alignItems: "center", flex: idx < STEPS.length - 1 ? 1 : undefined }}>
            {/* Circle */}
            <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 4, minWidth: 40 }}>
              <div
                style={{
                  width: 28,
                  height: 28,
                  borderRadius: "50%",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 12,
                  fontWeight: 700,
                  flexShrink: 0,
                  background: isComplete
                    ? "rgba(34, 197, 94, 0.85)"
                    : isActive
                    ? "rgba(124, 58, 237, 0.85)"
                    : "rgba(148, 163, 184, 0.15)",
                  color: isComplete || isActive ? "#fff" : "rgba(255,255,255,0.4)",
                  border: isActive ? "2px solid rgba(124,58,237,0.6)" : "2px solid transparent",
                  transition: "all 200ms",
                }}
              >
                {isComplete ? (
                  <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                    <path d="M2.5 6.5l3 3 5-5" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                ) : (
                  idx + 1
                )}
              </div>
              <span
                style={{
                  fontSize: 9,
                  fontWeight: isActive ? 700 : 500,
                  color: isActive ? "rgba(255,255,255,0.9)" : isComplete ? "rgba(34,197,94,0.8)" : "rgba(255,255,255,0.35)",
                  textAlign: "center",
                  whiteSpace: "nowrap",
                  letterSpacing: "0.02em",
                }}
              >
                {label}
              </span>
            </div>
            {/* Connector line */}
            {idx < STEPS.length - 1 && (
              <div
                style={{
                  flex: 1,
                  height: 2,
                  marginBottom: 18,
                  marginLeft: 2,
                  marginRight: 2,
                  background: isComplete
                    ? "rgba(34, 197, 94, 0.5)"
                    : "rgba(148, 163, 184, 0.15)",
                  transition: "background 300ms",
                }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

// ---- Step 1: Import Portfolio ----

function sanitizeTableName(filename: string): string {
  const base = filename.replace(/.*[\\/]/, "").replace(/\.[^.]+$/, "");
  let name = base.toLowerCase().replace(/[^a-z0-9]+/g, "_");
  name = name.replace(/^[0-9]+/, "");
  if (!name || name === "_") name = "table_" + Date.now();
  return name;
}

function Step1ImportPortfolio({ onNext }: { onNext: () => void }) {
  const tables = useAppStore((s) => s.tables);
  const addTable = useAppStore((s) => s.addTable);
  const updateTable = useAppStore((s) => s.updateTable);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const doneTables = tables.filter((t) => t.status === "done" || t.status === "ready");

  async function handleUpload() {
    if (!isTauri()) return;
    setError(null);

    const selected = await open({
      multiple: false,
      filters: [
        { name: "All Supported", extensions: ["csv", "geojson", "json"] },
        { name: "CSV", extensions: ["csv"] },
        { name: "GeoJSON", extensions: ["geojson", "json"] },
      ],
    });
    if (!selected) return;

    const filePath = Array.isArray(selected) ? selected[0] : selected;
    const tableName = sanitizeTableName(filePath);

    addTable({ name: tableName, status: "ingesting", progressMessage: "Starting...", progressPercent: 0, addressColumns: [] });
    setUploading(true);

    try {
      await invoke("ingest_file_pipeline", { csvPath: filePath, tableName });
      updateTable(tableName, { status: "done", progressMessage: undefined, progressPercent: 100 });
      onNext();
    } catch (err) {
      updateTable(tableName, { status: "error", error: String(err) });
      setError(String(err));
    } finally {
      setUploading(false);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <h3 style={{ fontSize: 15, fontWeight: 700, color: "#fff", margin: "0 0 6px" }}>Import Portfolio</h3>
        <p style={{ fontSize: 13, color: "rgba(255,255,255,0.55)", margin: 0, lineHeight: 1.5 }}>
          Upload a CSV or GeoJSON file containing your property portfolio. Spatia will ingest and clean the data automatically.
        </p>
      </div>

      {!isTauri() && (
        <div style={{ ...CARD_STYLE, borderColor: "rgba(251,191,36,0.3)", background: "rgba(251,191,36,0.06)" }}>
          <p style={{ fontSize: 12, color: "rgba(251,191,36,0.9)", margin: 0 }}>
            File upload requires the desktop app. Run <code style={{ fontFamily: "monospace" }}>pnpm tauri dev</code> to enable.
          </p>
        </div>
      )}

      {doneTables.length > 0 && (
        <div>
          <p style={{ ...LABEL_STYLE }}>Loaded tables</p>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {doneTables.map((t) => (
              <div key={t.name} style={{ ...CARD_STYLE, display: "flex", alignItems: "center", gap: 10 }}>
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                  <path d="M2 4h10M2 7h10M2 10h7" stroke="rgba(148,163,184,0.7)" strokeWidth="1.3" strokeLinecap="round" />
                </svg>
                <span style={{ fontSize: 13, color: "rgba(255,255,255,0.8)", flex: 1 }}>{t.name}</span>
                {t.rowCount != null && (
                  <span style={{ fontSize: 11, color: "rgba(148,163,184,0.6)" }}>{t.rowCount.toLocaleString()} rows</span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {error && (
        <p style={{ fontSize: 12, color: "rgba(239,68,68,0.9)", margin: 0 }}>{error}</p>
      )}

      <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
        {isTauri() && (
          <button
            onClick={() => void handleUpload()}
            disabled={uploading}
            style={uploading ? DISABLED_BTN : PRIMARY_BTN}
          >
            {uploading ? "Uploading..." : "+ Upload File"}
          </button>
        )}
        {doneTables.length > 0 && (
          <button onClick={onNext} style={SECONDARY_BTN}>
            Continue with existing data
          </button>
        )}
      </div>
    </div>
  );
}

// ---- Step 2: Review Geocoding ----

function Step2ReviewGeocoding({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const tables = useAppStore((s) => s.tables);

  const geocodedTables = tables.filter((t) => t.geocodeStats != null);
  const latestTable = geocodedTables[geocodedTables.length - 1];
  const stats = latestTable?.geocodeStats;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <h3 style={{ fontSize: 15, fontWeight: 700, color: "#fff", margin: "0 0 6px" }}>Review Geocoding</h3>
        <p style={{ fontSize: 13, color: "rgba(255,255,255,0.55)", margin: 0, lineHeight: 1.5 }}>
          Review the geocoding results for your portfolio data. Address-matched properties will appear on the map.
        </p>
      </div>

      {!stats ? (
        <div style={CARD_STYLE}>
          <p style={{ fontSize: 13, color: "rgba(255,255,255,0.5)", margin: 0 }}>
            No geocoding data found. You can geocode your data from the Tables panel, or continue to the next step.
          </p>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <p style={{ ...LABEL_STYLE }}>Results for: {latestTable.name}</p>

          {/* Top-level stats */}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 10 }}>
            <div style={CARD_STYLE}>
              <p style={LABEL_STYLE}>Total</p>
              <p style={VALUE_STYLE}>{stats.total.toLocaleString()}</p>
            </div>
            <div style={CARD_STYLE}>
              <p style={LABEL_STYLE}>Geocoded</p>
              <p style={{ ...VALUE_STYLE, color: "rgba(34,197,94,0.9)" }}>{stats.geocoded.toLocaleString()}</p>
            </div>
            <div style={CARD_STYLE}>
              <p style={LABEL_STYLE}>Unresolved</p>
              <p style={{ ...VALUE_STYLE, color: stats.unresolved > 0 ? "rgba(251,191,36,0.9)" : "rgba(255,255,255,0.9)" }}>
                {stats.unresolved.toLocaleString()}
              </p>
            </div>
          </div>

          {/* Coverage bar */}
          <div style={CARD_STYLE}>
            <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 6 }}>
              <span style={{ fontSize: 12, color: "rgba(255,255,255,0.6)" }}>Coverage</span>
              <span style={{ fontSize: 12, fontWeight: 600, color: "rgba(255,255,255,0.9)" }}>
                {stats.total > 0 ? Math.round((stats.geocoded / stats.total) * 100) : 0}%
              </span>
            </div>
            <div style={{ height: 6, borderRadius: 4, background: "rgba(148,163,184,0.15)", overflow: "hidden" }}>
              <div
                style={{
                  height: "100%",
                  width: `${stats.total > 0 ? (stats.geocoded / stats.total) * 100 : 0}%`,
                  background: "rgba(34,197,94,0.7)",
                  borderRadius: 4,
                  transition: "width 400ms ease",
                }}
              />
            </div>
          </div>

          {/* Source breakdown */}
          <div style={CARD_STYLE}>
            <p style={{ ...LABEL_STYLE, marginBottom: 8 }}>By source</p>
            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              {[
                { label: "Cached", value: stats.by_source.cache },
                { label: "Overture exact", value: stats.by_source.overture_exact },
                { label: "Overture fuzzy", value: stats.by_source.overture_fuzzy },
                { label: "Geocodio API", value: stats.by_source.geocodio },
              ]
                .filter((s) => s.value > 0)
                .map((s) => (
                  <div key={s.label} style={{ display: "flex", justifyContent: "space-between" }}>
                    <span style={{ fontSize: 12, color: "rgba(255,255,255,0.55)" }}>{s.label}</span>
                    <span style={{ fontSize: 12, fontWeight: 600, color: "rgba(255,255,255,0.8)" }}>{s.value.toLocaleString()}</span>
                  </div>
                ))}
            </div>
          </div>
        </div>
      )}

      <div style={{ display: "flex", gap: 10 }}>
        <button onClick={onBack} style={SECONDARY_BTN}>Back</button>
        <button onClick={onNext} style={PRIMARY_BTN}>Continue</button>
      </div>
    </div>
  );
}

// ---- Step 3: Load Risk Layers ----

type RiskLayerDef = {
  id: string;
  label: string;
  description: string;
  layerType: string;
  source: string;
  icon: React.ReactNode;
};

const RISK_LAYER_DEFS: RiskLayerDef[] = [
  {
    id: "fema_flood",
    label: "FEMA Flood Zones",
    description: "National Flood Hazard Layer — Special Flood Hazard Areas (SFHA)",
    layerType: "flood",
    source: "FEMA",
    icon: (
      <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
        <path d="M2 14c2-3 4-3 6 0s4 3 6 0" stroke="rgba(59,130,246,0.9)" strokeWidth="1.5" strokeLinecap="round" />
        <path d="M2 10c2-3 4-3 6 0s4 3 6 0" stroke="rgba(59,130,246,0.6)" strokeWidth="1.5" strokeLinecap="round" />
        <path d="M10 2v4M7.5 3.5l1.5 2 1.5-2" stroke="rgba(59,130,246,0.7)" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    ),
  },
  {
    id: "wildfire",
    label: "Wildfire Hazard",
    description: "Wildland-Urban Interface fire hazard severity zones",
    layerType: "wildfire",
    source: "CAL FIRE / USFS",
    icon: (
      <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
        <path d="M10 17C6.5 17 4 14.5 4 11c0-2 1-3.5 2.5-4.5C6 8 6.5 9.5 7.5 10c0-2.5 1.5-5 4-6.5C12 6 11 8 12.5 9c.5-1 1.5-2 2.5-2C16 8.5 16 10 16 11c0 3.5-2.5 6-6 6z" stroke="rgba(249,115,22,0.9)" strokeWidth="1.5" strokeLinejoin="round" />
      </svg>
    ),
  },
];

function Step3LoadRiskLayers({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const riskLayers = useAppStore((s) => s.riskLayers);
  const fetchRiskLayers = useAppStore((s) => s.fetchRiskLayers);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  async function handleLoadLayer(def: RiskLayerDef) {
    if (!isTauri()) return;

    const filePath = await open({
      multiple: false,
      filters: [
        { name: "All Supported", extensions: ["geojson", "json", "gpkg", "shp", "fgb"] },
        { name: "GeoJSON", extensions: ["geojson", "json"] },
        { name: "GeoPackage", extensions: ["gpkg"] },
      ],
    });
    if (!filePath) return;

    const path = Array.isArray(filePath) ? filePath[0] : filePath;
    setLoadingId(def.id);
    setErrors((prev) => { const next = { ...prev }; delete next[def.id]; return next; });

    try {
      await invoke("load_risk_layer", {
        filePath: path,
        layerName: def.id,
        displayName: def.label,
        layerType: def.layerType,
        source: def.source,
      });
      await fetchRiskLayers();
    } catch (err) {
      setErrors((prev) => ({ ...prev, [def.id]: String(err) }));
    } finally {
      setLoadingId(null);
    }
  }

  function isLoaded(defId: string): boolean {
    return riskLayers.some((l) => l.name === defId);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <h3 style={{ fontSize: 15, fontWeight: 700, color: "#fff", margin: "0 0 6px" }}>Load Risk Layers</h3>
        <p style={{ fontSize: 13, color: "rgba(255,255,255,0.55)", margin: 0, lineHeight: 1.5 }}>
          Load risk overlay data to assess exposure. Select a file for each hazard layer you want to include.
        </p>
      </div>

      {!isTauri() && (
        <div style={{ ...CARD_STYLE, borderColor: "rgba(251,191,36,0.3)", background: "rgba(251,191,36,0.06)" }}>
          <p style={{ fontSize: 12, color: "rgba(251,191,36,0.9)", margin: 0 }}>
            Risk layer loading requires the desktop app.
          </p>
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {RISK_LAYER_DEFS.map((def) => {
          const loaded = isLoaded(def.id);
          const loading = loadingId === def.id;
          const err = errors[def.id];

          return (
            <div
              key={def.id}
              style={{
                ...CARD_STYLE,
                display: "flex",
                alignItems: "center",
                gap: 14,
                borderColor: loaded
                  ? "rgba(34,197,94,0.25)"
                  : "rgba(148,163,184,0.12)",
                background: loaded
                  ? "rgba(34,197,94,0.05)"
                  : "rgba(255,255,255,0.04)",
              }}
            >
              <div style={{ flexShrink: 0 }}>{def.icon}</div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 2 }}>
                  <p style={{ fontSize: 13, fontWeight: 600, color: "rgba(255,255,255,0.9)", margin: 0 }}>{def.label}</p>
                  {loaded && (
                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                      <circle cx="7" cy="7" r="6" fill="rgba(34,197,94,0.2)" stroke="rgba(34,197,94,0.7)" strokeWidth="1.2" />
                      <path d="M4 7l2 2 4-4" stroke="rgba(34,197,94,0.9)" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  )}
                </div>
                <p style={{ fontSize: 11, color: "rgba(255,255,255,0.4)", margin: 0 }}>{def.description}</p>
                <p style={{ fontSize: 10, color: "rgba(148,163,184,0.5)", margin: "2px 0 0" }}>Source: {def.source}</p>
                {err && <p style={{ fontSize: 11, color: "rgba(239,68,68,0.85)", marginTop: 4 }}>{err}</p>}
              </div>
              {isTauri() && (
                <button
                  onClick={() => void handleLoadLayer(def)}
                  disabled={loading}
                  style={
                    loading
                      ? { ...DISABLED_BTN, flexShrink: 0 }
                      : loaded
                      ? { ...SECONDARY_BTN, flexShrink: 0 }
                      : { ...PRIMARY_BTN, flexShrink: 0 }
                  }
                >
                  {loading ? "Loading..." : loaded ? "Replace" : "Load File"}
                </button>
              )}
            </div>
          );
        })}
      </div>

      <div style={{ display: "flex", gap: 10 }}>
        <button onClick={onBack} style={SECONDARY_BTN}>Back</button>
        <button onClick={onNext} style={PRIMARY_BTN}>Continue</button>
      </div>
    </div>
  );
}

// ---- Step 4: View Assessment ----

function Step4ViewAssessment({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const tables = useAppStore((s) => s.tables);
  const riskLayers = useAppStore((s) => s.riskLayers);

  const doneTables = tables.filter((t) => t.status === "done" || t.status === "ready");
  const totalProperties = doneTables.reduce((sum, t) => sum + (t.rowCount ?? 0), 0);
  const geocodedCount = doneTables.reduce((sum, t) => sum + (t.geocodeStats?.geocoded ?? 0), 0);
  const loadedLayerCount = riskLayers.length;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <h3 style={{ fontSize: 15, fontWeight: 700, color: "#fff", margin: "0 0 6px" }}>View Assessment</h3>
        <p style={{ fontSize: 13, color: "rgba(255,255,255,0.55)", margin: 0, lineHeight: 1.5 }}>
          Summary of your portfolio and loaded risk layers. Use the AI chat to run spatial queries and risk analysis.
        </p>
      </div>

      {/* Portfolio summary */}
      <div>
        <p style={LABEL_STYLE}>Portfolio summary</p>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 10 }}>
          <div style={CARD_STYLE}>
            <p style={LABEL_STYLE}>Properties</p>
            <p style={VALUE_STYLE}>{totalProperties.toLocaleString()}</p>
            <p style={NOTE_STYLE}>{doneTables.length} table{doneTables.length !== 1 ? "s" : ""}</p>
          </div>
          <div style={CARD_STYLE}>
            <p style={LABEL_STYLE}>Geocoded</p>
            <p style={{ ...VALUE_STYLE, color: geocodedCount > 0 ? "rgba(34,197,94,0.9)" : "rgba(255,255,255,0.9)" }}>
              {geocodedCount.toLocaleString()}
            </p>
            <p style={NOTE_STYLE}>
              {totalProperties > 0 ? Math.round((geocodedCount / totalProperties) * 100) : 0}% coverage
            </p>
          </div>
          <div style={CARD_STYLE}>
            <p style={LABEL_STYLE}>Risk Layers</p>
            <p style={{ ...VALUE_STYLE, color: loadedLayerCount > 0 ? "rgba(251,191,36,0.9)" : "rgba(255,255,255,0.9)" }}>
              {loadedLayerCount}
            </p>
            <p style={NOTE_STYLE}>loaded</p>
          </div>
        </div>
      </div>

      {/* Risk layers summary */}
      {riskLayers.length > 0 && (
        <div>
          <p style={LABEL_STYLE}>Risk layers</p>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {riskLayers.map((layer) => (
              <div key={layer.name} style={{ ...CARD_STYLE, display: "flex", alignItems: "center", gap: 12 }}>
                <div
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: layer.layer_type === "flood" ? "rgba(59,130,246,0.8)" : "rgba(249,115,22,0.8)",
                    flexShrink: 0,
                  }}
                />
                <div style={{ flex: 1 }}>
                  <p style={{ fontSize: 13, color: "rgba(255,255,255,0.85)", margin: 0 }}>{layer.display_name}</p>
                  <p style={{ fontSize: 11, color: "rgba(148,163,184,0.55)", margin: 0 }}>
                    {layer.row_count.toLocaleString()} features · {layer.source}
                  </p>
                </div>
                <span
                  style={{
                    fontSize: 10,
                    fontWeight: 600,
                    padding: "2px 8px",
                    borderRadius: 4,
                    background: layer.has_geometry ? "rgba(34,197,94,0.15)" : "rgba(148,163,184,0.1)",
                    color: layer.has_geometry ? "rgba(34,197,94,0.8)" : "rgba(148,163,184,0.6)",
                  }}
                >
                  {layer.has_geometry ? "Spatial" : "Tabular"}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Tables list */}
      {doneTables.length > 0 && (
        <div>
          <p style={LABEL_STYLE}>Portfolio tables</p>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {doneTables.map((t) => (
              <div key={t.name} style={{ ...CARD_STYLE, display: "flex", alignItems: "center", gap: 10 }}>
                <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                  <path d="M1.5 3.5h10M1.5 6.5h10M1.5 9.5h6" stroke="rgba(148,163,184,0.6)" strokeWidth="1.2" strokeLinecap="round" />
                </svg>
                <span style={{ fontSize: 13, color: "rgba(255,255,255,0.8)", flex: 1 }}>{t.name}</span>
                <span style={{ fontSize: 11, color: "rgba(148,163,184,0.5)" }}>
                  {(t.rowCount ?? 0).toLocaleString()} rows
                </span>
                {t.geocodeStats && (
                  <span style={{ fontSize: 10, color: "rgba(34,197,94,0.7)" }}>
                    {t.geocodeStats.geocoded}/{t.geocodeStats.total} geocoded
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {doneTables.length === 0 && (
        <div style={CARD_STYLE}>
          <p style={{ fontSize: 13, color: "rgba(255,255,255,0.45)", margin: 0 }}>
            No portfolio data loaded yet. Go back to import a file.
          </p>
        </div>
      )}

      <div style={{ display: "flex", gap: 10 }}>
        <button onClick={onBack} style={SECONDARY_BTN}>Back</button>
        <button onClick={onNext} style={PRIMARY_BTN}>Export Results</button>
      </div>
    </div>
  );
}

// ---- Step 5: Export Results ----

function Step5Export({ onBack, onClose }: { onBack: () => void; onClose: () => void }) {
  const tables = useAppStore((s) => s.tables);
  const [exporting, setExporting] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);

  const doneTables = tables.filter((t) => t.status === "done" || t.status === "ready");

  async function handleExportCsv() {
    if (!isTauri() || doneTables.length === 0) return;
    const target = doneTables[doneTables.length - 1];

    const filePath = await save({
      defaultPath: `${target.name}_risk_assessment.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!filePath) return;

    setExporting("csv");
    try {
      await invoke("export_table_csv", { tableName: target.name, filePath });
      setFeedback("CSV exported successfully.");
    } catch (err) {
      setFeedback(`Export failed: ${err}`);
    } finally {
      setExporting(null);
      setTimeout(() => setFeedback(null), 3000);
    }
  }

  async function handleExportGeoJson() {
    if (!isTauri()) return;

    const filePath = await save({
      defaultPath: "analysis_result.geojson",
      filters: [{ name: "GeoJSON", extensions: ["geojson"] }],
    });
    if (!filePath) return;

    setExporting("geojson");
    try {
      await invoke("export_analysis_geojson", { filePath });
      setFeedback("GeoJSON exported successfully.");
    } catch (err) {
      setFeedback(`Export failed: ${err}`);
    } finally {
      setExporting(null);
      setTimeout(() => setFeedback(null), 3000);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <h3 style={{ fontSize: 15, fontWeight: 700, color: "#fff", margin: "0 0 6px" }}>Export Results</h3>
        <p style={{ fontSize: 13, color: "rgba(255,255,255,0.55)", margin: 0, lineHeight: 1.5 }}>
          Export your risk assessment results for use in reports or downstream systems.
        </p>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {/* CSV export */}
        <div
          style={{
            ...CARD_STYLE,
            display: "flex",
            alignItems: "center",
            gap: 14,
            opacity: doneTables.length === 0 ? 0.5 : 1,
          }}
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
            <path d="M5 3h8l3 3v11a1 1 0 01-1 1H5a1 1 0 01-1-1V4a1 1 0 011-1z" stroke="rgba(34,197,94,0.7)" strokeWidth="1.4" strokeLinejoin="round" />
            <path d="M13 3v4h4M7 10h6M7 13h4" stroke="rgba(34,197,94,0.6)" strokeWidth="1.3" strokeLinecap="round" />
          </svg>
          <div style={{ flex: 1 }}>
            <p style={{ fontSize: 13, fontWeight: 600, color: "rgba(255,255,255,0.9)", margin: "0 0 2px" }}>Export CSV</p>
            <p style={{ fontSize: 11, color: "rgba(255,255,255,0.4)", margin: 0 }}>
              {doneTables.length > 0
                ? `Export most recent table: ${doneTables[doneTables.length - 1].name}`
                : "No portfolio data loaded"}
            </p>
          </div>
          <button
            onClick={() => void handleExportCsv()}
            disabled={!isTauri() || exporting !== null || doneTables.length === 0}
            style={
              !isTauri() || doneTables.length === 0 || exporting !== null
                ? { ...DISABLED_BTN, flexShrink: 0 }
                : { ...PRIMARY_BTN, flexShrink: 0 }
            }
          >
            {exporting === "csv" ? "Exporting..." : "Export"}
          </button>
        </div>

        {/* GeoJSON export */}
        <div style={{ ...CARD_STYLE, display: "flex", alignItems: "center", gap: 14 }}>
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
            <circle cx="10" cy="10" r="7" stroke="rgba(99,102,241,0.7)" strokeWidth="1.4" />
            <path d="M3 10h14M10 3c-2 2-3 4.5-3 7s1 5 3 7M10 3c2 2 3 4.5 3 7s-1 5-3 7" stroke="rgba(99,102,241,0.6)" strokeWidth="1.3" strokeLinecap="round" />
          </svg>
          <div style={{ flex: 1 }}>
            <p style={{ fontSize: 13, fontWeight: 600, color: "rgba(255,255,255,0.9)", margin: "0 0 2px" }}>Export GeoJSON</p>
            <p style={{ fontSize: 11, color: "rgba(255,255,255,0.4)", margin: 0 }}>
              Export current analysis result as GeoJSON
            </p>
          </div>
          <button
            onClick={() => void handleExportGeoJson()}
            disabled={!isTauri() || exporting !== null}
            style={
              !isTauri() || exporting !== null
                ? { ...DISABLED_BTN, flexShrink: 0 }
                : { ...SECONDARY_BTN, flexShrink: 0 }
            }
          >
            {exporting === "geojson" ? "Exporting..." : "Export"}
          </button>
        </div>
      </div>

      {feedback && (
        <p
          style={{
            fontSize: 12,
            color: feedback.startsWith("Export failed") ? "rgba(239,68,68,0.9)" : "rgba(34,197,94,0.9)",
            margin: 0,
          }}
        >
          {feedback}
        </p>
      )}

      <div style={{ display: "flex", gap: 10 }}>
        <button onClick={onBack} style={SECONDARY_BTN}>Back</button>
        <button
          onClick={onClose}
          style={{ ...PRIMARY_BTN, background: "rgba(34,197,94,0.6)" }}
        >
          Done
        </button>
      </div>
    </div>
  );
}

// ---- Main RiskWorkflow component ----

export function RiskWorkflow({ open: isOpen, onClose }: Props) {
  const workflowStep = useAppStore((s) => s.workflowStep);
  const setWorkflowStep = useAppStore((s) => s.setWorkflowStep);

  function goNext() { setWorkflowStep(Math.min(workflowStep + 1, STEPS.length - 1)); }
  function goBack() { setWorkflowStep(Math.max(workflowStep - 1, 0)); }

  function handleClose() {
    setWorkflowStep(0);
    onClose();
  }

  if (!isOpen) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 200,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {/* Backdrop */}
      <div
        onClick={handleClose}
        style={{
          position: "absolute",
          inset: 0,
          background: "rgba(0,0,0,0.65)",
          backdropFilter: "blur(5px)",
        }}
      />

      {/* Panel */}
      <div style={PANEL_STYLE}>
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 20 }}>
          <div>
            <h2 style={{ fontSize: 18, fontWeight: 700, color: "#fff", margin: "0 0 2px" }}>
              Risk Assessment Workflow
            </h2>
            <p style={{ fontSize: 12, color: "rgba(255,255,255,0.4)", margin: 0 }}>
              Step {workflowStep + 1} of {STEPS.length}
            </p>
          </div>
          <button
            onClick={handleClose}
            title="Close workflow"
            style={{
              background: "none",
              border: "none",
              color: "rgba(255,255,255,0.45)",
              fontSize: 20,
              cursor: "pointer",
              padding: "0 4px",
              lineHeight: 1,
              marginTop: 2,
            }}
          >
            &times;
          </button>
        </div>

        {/* Step indicator */}
        <StepIndicator currentStep={workflowStep} />

        {/* Step content */}
        {workflowStep === 0 && <Step1ImportPortfolio onNext={goNext} />}
        {workflowStep === 1 && <Step2ReviewGeocoding onNext={goNext} onBack={goBack} />}
        {workflowStep === 2 && <Step3LoadRiskLayers onNext={goNext} onBack={goBack} />}
        {workflowStep === 3 && <Step4ViewAssessment onNext={goNext} onBack={goBack} />}
        {workflowStep === 4 && <Step5Export onBack={goBack} onClose={handleClose} />}
      </div>
    </div>
  );
}
