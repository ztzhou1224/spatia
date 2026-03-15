import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type RecoverResult = {
  success: boolean;
  message: string;
  backup_path: string | null;
};

type Props = {
  error: string;
  fileSizeBytes: number;
};

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function DbRecoveryDialog({ error, fileSizeBytes }: Props) {
  const [recovering, setRecovering] = useState(false);
  const [result, setResult] = useState<RecoverResult | null>(null);
  const [recoverError, setRecoverError] = useState<string | null>(null);

  async function handleRecover(action: "BackupAndRecreate" | "DeleteAndRecreate") {
    setRecovering(true);
    setRecoverError(null);
    try {
      const res = await invoke<RecoverResult>("recover_db_cmd", { action });
      setResult(res);
      if (res.success) {
        // Brief pause so the user can read the success message, then reload
        setTimeout(() => {
          window.location.reload();
        }, 2500);
      }
    } catch (err) {
      setRecoverError(String(err));
    } finally {
      setRecovering(false);
    }
  }

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 200,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0, 0, 0, 0.75)",
        backdropFilter: "blur(6px)",
      }}
    >
      <div
        style={{
          position: "relative",
          width: 480,
          maxWidth: "90vw",
          background: "rgba(20, 20, 28, 0.97)",
          border: "1px solid rgba(239, 68, 68, 0.35)",
          borderRadius: 12,
          padding: 28,
          boxShadow: "0 20px 60px rgba(0,0,0,0.8)",
        }}
      >
        {/* Header */}
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 16 }}>
          <span style={{ fontSize: 20 }}>&#9888;&#xFE0F;</span>
          <h2 style={{ fontSize: 17, fontWeight: 700, color: "#fff", margin: 0 }}>
            Database Corrupted
          </h2>
        </div>

        {/* Error detail */}
        <div
          style={{
            background: "rgba(239, 68, 68, 0.08)",
            border: "1px solid rgba(239, 68, 68, 0.2)",
            borderRadius: 8,
            padding: "12px 14px",
            marginBottom: 14,
          }}
        >
          <p style={{ fontSize: 12, color: "rgba(239, 68, 68, 0.9)", margin: 0, wordBreak: "break-word" }}>
            {error}
          </p>
        </div>

        {fileSizeBytes > 0 && (
          <p style={{ fontSize: 11, color: "rgba(255,255,255,0.45)", marginBottom: 20 }}>
            Corrupt file size: {formatBytes(fileSizeBytes)}
          </p>
        )}

        {/* Success state */}
        {result?.success && (
          <div
            style={{
              background: "rgba(34, 197, 94, 0.1)",
              border: "1px solid rgba(34, 197, 94, 0.25)",
              borderRadius: 8,
              padding: "12px 14px",
              marginBottom: 16,
            }}
          >
            <p style={{ fontSize: 13, color: "rgba(34, 197, 94, 0.95)", margin: 0 }}>
              {result.message}
            </p>
            {result.backup_path && (
              <p style={{ fontSize: 11, color: "rgba(255,255,255,0.45)", marginTop: 6, margin: "6px 0 0", wordBreak: "break-all" }}>
                Backup saved to: {result.backup_path}
              </p>
            )}
            <p style={{ fontSize: 11, color: "rgba(255,255,255,0.4)", marginTop: 6, margin: "6px 0 0" }}>
              Reloading the app...
            </p>
          </div>
        )}

        {/* Invoke error */}
        {recoverError && (
          <p style={{ fontSize: 12, color: "rgba(239, 68, 68, 0.9)", marginBottom: 14 }}>
            Recovery failed: {recoverError}
          </p>
        )}

        {/* Failure result */}
        {result && !result.success && (
          <p style={{ fontSize: 12, color: "rgba(239, 68, 68, 0.9)", marginBottom: 14 }}>
            {result.message}
          </p>
        )}

        {/* Action buttons — hide once recovery succeeded */}
        {!result?.success && (
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <button
              onClick={() => void handleRecover("BackupAndRecreate")}
              disabled={recovering}
              style={{
                padding: "10px 16px",
                fontSize: 13,
                fontWeight: 600,
                background: recovering ? "rgba(124, 58, 237, 0.25)" : "rgba(124, 58, 237, 0.65)",
                color: recovering ? "rgba(255,255,255,0.45)" : "#fff",
                border: "none",
                borderRadius: 8,
                cursor: recovering ? "default" : "pointer",
                textAlign: "left",
              }}
            >
              {recovering ? "Working..." : "Save corrupt file and start fresh"}
              {!recovering && (
                <span
                  style={{
                    display: "block",
                    fontSize: 11,
                    fontWeight: 400,
                    color: "rgba(255,255,255,0.6)",
                    marginTop: 2,
                  }}
                >
                  Recommended — keeps a backup of the corrupt file before creating a new database
                </span>
              )}
            </button>

            <button
              onClick={() => void handleRecover("DeleteAndRecreate")}
              disabled={recovering}
              style={{
                padding: "10px 16px",
                fontSize: 13,
                fontWeight: 600,
                background: "rgba(239, 68, 68, 0.12)",
                color: recovering ? "rgba(239, 68, 68, 0.35)" : "rgba(239, 68, 68, 0.85)",
                border: "1px solid rgba(239, 68, 68, 0.2)",
                borderRadius: 8,
                cursor: recovering ? "default" : "pointer",
                textAlign: "left",
              }}
            >
              {recovering ? "Working..." : "Delete and start fresh"}
              {!recovering && (
                <span
                  style={{
                    display: "block",
                    fontSize: 11,
                    fontWeight: 400,
                    color: "rgba(239,68,68,0.55)",
                    marginTop: 2,
                  }}
                >
                  Permanently deletes the corrupt file — no backup is kept
                </span>
              )}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
