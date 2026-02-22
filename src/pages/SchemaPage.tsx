import { useState, useEffect } from "react";
import { safeInvoke, isTauri } from "../lib/tauri";
import { MOCK_TABLES, type TableColumn } from "../mock/data";

export function SchemaPage() {
  const [dbPath, setDbPath] = useState("./spatia.duckdb");
  const [tableName, setTableName] = useState("places");
  const [columns, setColumns] = useState<TableColumn[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load mock schema on first render so the page isn't empty.
  useEffect(() => {
    void loadSchema();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function loadSchema() {
    setLoading(true);
    setError(null);

    const mockTable = MOCK_TABLES.find((t) => t.name === tableName);
    const mockFallback = JSON.stringify(mockTable?.columns ?? []);

    // BLOCKER: Requires Tauri backend.
    //   Real call: invoke("execute_engine_command", {
    //     command: `schema ${dbPath} ${tableName}`
    //   })
    //   Returns JSON string: array of { name: string, type: string, nullable: boolean }
    //   See engine schema.rs / executor.rs for the full schema command surface.
    const raw = await safeInvoke<string>(
      "execute_engine_command",
      { command: `schema ${dbPath} ${tableName}` },
      mockFallback,
    );

    if (!raw) {
      setError("No response from engine.");
    } else {
      try {
        setColumns(JSON.parse(raw) as TableColumn[]);
      } catch {
        setError(`Unexpected response: ${raw}`);
      }
    }

    setLoading(false);
  }

  return (
    <div className="page-content">
      <h2>Schema Viewer</h2>
      <p className="page-description">
        Inspect the columns of a DuckDB table via the engine's{" "}
        <code>schema</code> command.
        {!isTauri() && (
          <span className="demo-note">
            {" "}
            (Demo mode — mock schema shown for <em>{tableName}</em>)
          </span>
        )}
      </p>

      <form
        className="schema-form"
        onSubmit={(e) => {
          e.preventDefault();
          void loadSchema();
        }}
      >
        <label>
          Database path
          <input
            type="text"
            value={dbPath}
            onChange={(e) => setDbPath(e.target.value)}
            placeholder="./spatia.duckdb"
          />
        </label>

        <label>
          Table name
          <input
            type="text"
            value={tableName}
            onChange={(e) => setTableName(e.target.value)}
            placeholder="places"
          />
        </label>

        <button type="submit" disabled={loading}>
          {loading ? "Loading…" : "Load schema"}
        </button>
      </form>

      {error && <p className="error-msg">{error}</p>}

      {columns && columns.length === 0 && (
        <p className="empty-msg">No columns found for table "{tableName}".</p>
      )}

      {columns && columns.length > 0 && (
        <table className="schema-table">
          <thead>
            <tr>
              <th>Column</th>
              <th>Type</th>
              <th>Nullable</th>
            </tr>
          </thead>
          <tbody>
            {columns.map((col) => (
              <tr key={col.name}>
                <td>{col.name}</td>
                <td>
                  <code>{col.type}</code>
                </td>
                <td>{col.nullable ? "yes" : "no"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
