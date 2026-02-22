import { useState } from "react";
import { safeInvoke, isTauri } from "../lib/tauri";
import { MOCK_GEOCODE_RESULTS, type GeocodeResult } from "../mock/data";

export function SearchPage() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<GeocodeResult[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setResults(null);

    // Filter mock results by query string for a realistic demo experience.
    const filtered = MOCK_GEOCODE_RESULTS.filter((r) =>
      r.address.toLowerCase().includes(query.toLowerCase()),
    );
    const mockFallback = JSON.stringify(
      filtered.length > 0 ? filtered : MOCK_GEOCODE_RESULTS,
    );

    // BLOCKER: Requires Tauri backend.
    //   Current (transitional) call:
    //     invoke("execute_engine_command", { command: `geocode "${query}"` })
    //     Returns JSON array of { address, lat, lon, source }
    //
    //   Long-term: Replace with Overture-based place search once the local
    //   DuckDB tables are populated via overture_extract:
    //     invoke("execute_engine_command", {
    //       command: `overture_search ./spatia.duckdb places_wa "${query}" 20`
    //     })
    const raw = await safeInvoke<string>(
      "execute_engine_command",
      { command: `geocode "${query}"` },
      mockFallback,
    );

    if (!raw) {
      setError("No response from engine.");
    } else {
      try {
        setResults(JSON.parse(raw) as GeocodeResult[]);
      } catch {
        setError(`Unexpected response: ${raw}`);
      }
    }

    setLoading(false);
  }

  return (
    <div className="page-content">
      <h2>Search / Geocode</h2>
      <p className="page-description">
        Look up an address or place name.
        {!isTauri() && (
          <span className="demo-note">
            {" "}
            (Demo mode — mock geocode results shown)
          </span>
        )}
      </p>

      <form className="search-form" onSubmit={handleSearch}>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="e.g. San Francisco, CA"
          className="search-input"
          required
        />
        <button type="submit" disabled={loading || !query.trim()}>
          {loading ? "Searching…" : "Search"}
        </button>
      </form>

      {error && <p className="error-msg">{error}</p>}

      {results && results.length === 0 && (
        <p className="empty-msg">No results found for "{query}".</p>
      )}

      {results && results.length > 0 && (
        <table className="results-table">
          <thead>
            <tr>
              <th>Address</th>
              <th>Latitude</th>
              <th>Longitude</th>
              <th>Source</th>
            </tr>
          </thead>
          <tbody>
            {results.map((r) => (
              <tr key={`${r.address}-${r.lat}-${r.lon}`}>
                <td>{r.address}</td>
                <td>{r.lat.toFixed(4)}</td>
                <td>{r.lon.toFixed(4)}</td>
                <td>
                  <span className="source-badge">{r.source}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
