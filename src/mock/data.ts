/** Mock data used when the Tauri backend is not available. */

export interface TableColumn {
  name: string;
  type: string;
  nullable: boolean;
}

export interface MockTable {
  name: string;
  columns: TableColumn[];
}

export const MOCK_TABLES: MockTable[] = [
  {
    name: "places",
    columns: [
      { name: "id", type: "INTEGER", nullable: false },
      { name: "name", type: "VARCHAR", nullable: true },
      { name: "lat", type: "DOUBLE", nullable: true },
      { name: "lon", type: "DOUBLE", nullable: true },
      { name: "category", type: "VARCHAR", nullable: true },
    ],
  },
  {
    name: "raw_staging",
    columns: [
      { name: "row_num", type: "BIGINT", nullable: false },
      { name: "address", type: "VARCHAR", nullable: true },
      { name: "city", type: "VARCHAR", nullable: true },
      { name: "state", type: "VARCHAR", nullable: true },
    ],
  },
];

export interface GeocodeResult {
  address: string;
  lat: number;
  lon: number;
  source: string;
}

export const MOCK_GEOCODE_RESULTS: GeocodeResult[] = [
  { address: "San Francisco, CA", lat: 37.7749, lon: -122.4194, source: "mock" },
  { address: "Seattle, WA", lat: 47.6062, lon: -122.3321, source: "mock" },
  { address: "New York, NY", lat: 40.7128, lon: -74.006, source: "mock" },
];

export interface IngestResult {
  status: string;
  table: string;
}

/** Mock result returned by the ingest form when running without backend. */
export const MOCK_INGEST_RESULT: IngestResult = {
  status: "ok",
  table: "raw_staging",
};
