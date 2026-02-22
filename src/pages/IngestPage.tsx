import { useState } from "react";
import { safeInvoke, isTauri } from "../lib/tauri";
import { type IngestResult, MOCK_INGEST_RESULT } from "../mock/data";

export function IngestPage() {
  const [dbPath, setDbPath] = useState("./spatia.duckdb");
  const [csvPath, setCsvPath] = useState("");
  const [tableName, setTableName] = useState("raw_staging");
  const [result, setResult] = useState<IngestResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setResult(null);

    // BLOCKER: Requires Tauri backend.
    //   Real call: invoke("execute_engine_command", {
    //     command: `ingest ${dbPath} ${csvPath} ${tableName}`
    //   })
    //   Returns JSON string: { "status": "ok", "table": "<name>" }
    //   See engine executor.rs for the full ingest command surface.
    const raw = await safeInvoke<string>(
      "execute_engine_command",
      { command: `ingest ${dbPath} ${csvPath} ${tableName}` },
      JSON.stringify({ ...MOCK_INGEST_RESULT, table: tableName }),
    );

    if (!raw) {
      setError("No response from engine.");
    } else {
      try {
        setResult(JSON.parse(raw) as IngestResult);
      } catch {
        setError(`Unexpected response: ${raw}`);
      }
    }

    setLoading(false);
  }

  return (
    <div className="page-content">
      <h2>Ingest CSV</h2>
      <p className="page-description">
        Load a CSV file into a DuckDB table via the engine's{" "}
        <code>ingest</code> command.
        {!isTauri() && (
          <span className="demo-note">
            {" "}
            (Demo mode — a mock result will be shown)
          </span>
        )}
      </p>

      <form className="ingest-form" onSubmit={handleSubmit}>
        <label>
          Database path
          <input
            type="text"
            value={dbPath}
            onChange={(e) => setDbPath(e.target.value)}
            placeholder="./spatia.duckdb"
            required
          />
        </label>

        <label>
          CSV file path
          <input
            type="text"
            value={csvPath}
            onChange={(e) => setCsvPath(e.target.value)}
            placeholder="./data/sample.csv"
            required
          />
        </label>

        <label>
          Table name
          <input
            type="text"
            value={tableName}
            onChange={(e) => setTableName(e.target.value)}
            placeholder="raw_staging"
            required
          />
        </label>

        <button type="submit" disabled={loading}>
          {loading ? "Ingesting…" : "Ingest"}
        </button>
      </form>

      {error && <p className="error-msg">{error}</p>}

      {result && (
        <div className="result-box">
          <p>
            ✅ Table <strong>{result.table}</strong> loaded successfully
            (status: {result.status}){!isTauri() && " [mock]"}
          </p>
        </div>
      )}
    </div>
  );
}
