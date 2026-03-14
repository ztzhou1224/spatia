import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../lib/appStore";

type Props = {
  open: boolean;
  onClose: () => void;
};

function ApiKeyRow({
  label,
  keyName,
  onSaved,
}: {
  label: string;
  keyName: string;
  onSaved: () => void;
}) {
  const [value, setValue] = useState("");
  const [hasStored, setHasStored] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);

  useEffect(() => {
    invoke<string | null>("get_api_key", { keyName }).then((val) => {
      if (val) {
        setHasStored(true);
      }
    }).catch(() => {});
  }, [keyName]);

  async function handleSave() {
    if (!value.trim()) return;
    try {
      await invoke("save_api_key", { keyName, keyValue: value.trim() });
      setHasStored(true);
      setValue("");
      setFeedback("Saved");
      onSaved();
      setTimeout(() => setFeedback(null), 2000);
    } catch (err) {
      setFeedback(`Error: ${err}`);
    }
  }

  async function handleDelete() {
    try {
      await invoke("delete_api_key", { keyName });
      setHasStored(false);
      setValue("");
      setFeedback("Removed");
      onSaved();
      setTimeout(() => setFeedback(null), 2000);
    } catch (err) {
      setFeedback(`Error: ${err}`);
    }
  }

  return (
    <div style={{ marginBottom: 16 }}>
      <label style={{ fontSize: 12, fontWeight: 600, color: "rgba(255,255,255,0.85)", display: "block", marginBottom: 4 }}>
        {label}
      </label>
      <div style={{ display: "flex", gap: 6 }}>
        <input
          type="password"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={hasStored ? "••••••••  (stored)" : "Enter API key..."}
          style={{
            flex: 1,
            padding: "6px 10px",
            fontSize: 12,
            background: "rgba(255,255,255,0.06)",
            border: "1px solid rgba(148, 163, 184, 0.2)",
            borderRadius: 6,
            color: "#fff",
            outline: "none",
          }}
          onKeyDown={(e) => { if (e.key === "Enter") void handleSave(); }}
        />
        <button
          onClick={() => void handleSave()}
          disabled={!value.trim()}
          style={{
            padding: "6px 12px",
            fontSize: 11,
            fontWeight: 600,
            background: value.trim() ? "rgba(124, 58, 237, 0.6)" : "rgba(124, 58, 237, 0.2)",
            color: value.trim() ? "#fff" : "rgba(255,255,255,0.4)",
            border: "none",
            borderRadius: 6,
            cursor: value.trim() ? "pointer" : "default",
          }}
        >
          Save
        </button>
        {hasStored && (
          <button
            onClick={() => void handleDelete()}
            title="Remove stored key"
            style={{
              padding: "6px 10px",
              fontSize: 11,
              background: "rgba(239, 68, 68, 0.15)",
              color: "rgba(239, 68, 68, 0.8)",
              border: "none",
              borderRadius: 6,
              cursor: "pointer",
            }}
          >
            Remove
          </button>
        )}
      </div>
      {feedback && (
        <p style={{ fontSize: 11, marginTop: 4, color: feedback.startsWith("Error") ? "rgba(239,68,68,0.9)" : "rgba(34,197,94,0.9)" }}>
          {feedback}
        </p>
      )}
    </div>
  );
}

export function SettingsPanel({ open, onClose }: Props) {
  const fetchApiConfig = useAppStore((s) => s.fetchApiConfig);

  if (!open) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 100,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {/* Backdrop */}
      <div
        onClick={onClose}
        style={{
          position: "absolute",
          inset: 0,
          background: "rgba(0,0,0,0.6)",
          backdropFilter: "blur(4px)",
        }}
      />
      {/* Panel */}
      <div
        style={{
          position: "relative",
          width: 420,
          maxWidth: "90vw",
          background: "rgba(20, 20, 28, 0.95)",
          border: "1px solid rgba(148, 163, 184, 0.15)",
          borderRadius: 12,
          padding: 24,
          boxShadow: "0 20px 60px rgba(0,0,0,0.7)",
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 20 }}>
          <h2 style={{ fontSize: 16, fontWeight: 700, color: "#fff", margin: 0 }}>Settings</h2>
          <button
            onClick={onClose}
            title="Close settings"
            style={{
              background: "none",
              border: "none",
              color: "rgba(255,255,255,0.5)",
              fontSize: 18,
              cursor: "pointer",
              padding: 4,
            }}
          >
            &times;
          </button>
        </div>

        <div style={{ fontSize: 11, color: "rgba(255,255,255,0.5)", marginBottom: 16 }}>
          API keys are stored locally on your machine. Environment variables take priority if set.
        </div>

        <ApiKeyRow
          label="Gemini API Key"
          keyName="gemini_api_key"
          onSaved={() => void fetchApiConfig()}
        />
        <ApiKeyRow
          label="Geocodio API Key (optional)"
          keyName="geocodio_api_key"
          onSaved={() => void fetchApiConfig()}
        />
      </div>
    </div>
  );
}
