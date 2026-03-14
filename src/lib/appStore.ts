import { create, type StateCreator } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

export type GeocodeStats = {
  total: number;
  geocoded: number;
  by_source: {
    cache: number;
    overture_fuzzy: number;
    geocodio: number;
  };
  unresolved: number;
};

export type TableInfo = {
  name: string;
  rowCount?: number;
  status: "ingesting" | "cleaning" | "detecting" | "ready" | "geocoding" | "done" | "error";
  progressMessage?: string;
  progressPercent?: number;
  cleanSummary?: string;
  addressColumns: string[];
  geocodeColumn?: string;
  geocodeWarning?: string;
  geocodeStats?: GeocodeStats;
  error?: string;
};

export type ResultRows = {
  columns: string[];
  rows: (string | null)[][];
  truncated: boolean;
};

export type WidgetType = "table" | "bar_chart" | "pie_chart" | "histogram";

export type ActiveWidget = {
  type: WidgetType;
  title: string;
  data: ResultRows;
};

export type ChatMessage = {
  role: "user" | "assistant";
  content: string;
  sql?: string;
  rowCount?: number;
  resultRows?: ResultRows;
  retryAttempted?: boolean;
};

export type ApiConfig = {
  gemini: boolean;
  geocodio: boolean;
};

export type DomainPackConfig = {
  id: string;
  display_name: string;
  assistant_name: string;
  ui_config: {
    placeholder_no_data: string;
    placeholder_no_selection: string;
    placeholder_ready: string;
    empty_state_title: string;
    empty_state_description: string;
    upload_instruction: string;
    primary_color: string;
    map_default_center: [number, number];
    map_default_zoom: number;
  };
};

const DEFAULT_DOMAIN_CONFIG: DomainPackConfig = {
  id: "generic",
  display_name: "Generic GIS",
  assistant_name: "Spatia",
  ui_config: {
    placeholder_no_data: "Upload data to get started...",
    placeholder_no_selection: "Select tables to add context...",
    placeholder_ready: "Ask about your data...",
    empty_state_title: "No data yet",
    empty_state_description: "Spatia analyzes your location data with AI",
    upload_instruction:
      "Upload a CSV with addresses to get started. Spatia will clean the data, geocode the locations, and plot them on the map.",
    primary_color: "#7c3aed",
    map_default_center: [-122.4194, 37.7749],
    map_default_zoom: 11,
  },
};

type AppStore = {
  tables: TableInfo[];
  chatMessages: ChatMessage[];
  isProcessing: boolean;
  analysisGeoJson: unknown;
  visualizationType: string;
  tableGeoJson: Record<string, unknown>;
  mapActions: unknown[];
  apiConfig: ApiConfig | null;
  selectedTablesForChat: Set<string>;
  logPath: string | null;
  activeWidget: ActiveWidget | null;
  domainConfig: DomainPackConfig;
  basemapId: string;
  settingsOpen: boolean;
  analysisTotalCount: number | null;

  addTable: (table: TableInfo) => void;
  updateTable: (name: string, patch: Partial<TableInfo>) => void;
  removeTable: (name: string) => void;
  setTables: (tables: TableInfo[]) => void;
  addMessage: (message: ChatMessage) => void;
  clearMessages: () => void;
  setIsProcessing: (value: boolean) => void;
  setAnalysisGeoJson: (geojson: unknown) => void;
  setVisualizationType: (type: string) => void;
  setTableGeoJson: (tableName: string, geojson: unknown) => void;
  clearTableGeoJson: (tableName: string) => void;
  setMapActions: (actions: unknown[]) => void;
  fetchApiConfig: () => Promise<void>;
  fetchLogPath: () => Promise<void>;
  toggleTableForChat: (tableName: string) => void;
  setActiveWidget: (widget: ActiveWidget) => void;
  clearActiveWidget: () => void;
  selectAllTablesForChat: () => void;
  deselectAllTablesForChat: () => void;
  fetchDomainConfig: () => Promise<void>;
  setBasemapId: (id: string) => void;
  setSettingsOpen: (open: boolean) => void;
  setAnalysisTotalCount: (count: number | null) => void;
};

const storeInitializer: StateCreator<AppStore> = (set) => ({
  tables: [],
  chatMessages: [],
  isProcessing: false,
  analysisGeoJson: { type: "FeatureCollection", features: [] },
  visualizationType: "scatter",
  tableGeoJson: {},
  mapActions: [],
  apiConfig: null,
  logPath: null,
  selectedTablesForChat: new Set<string>(),
  activeWidget: null,
  domainConfig: DEFAULT_DOMAIN_CONFIG,
  basemapId: (typeof localStorage !== "undefined" ? localStorage.getItem("basemapId") : null) ?? "dark",
  settingsOpen: false,
  analysisTotalCount: null,

  addTable: (table) =>
    set((state) => ({ tables: [...state.tables, table] })),
  updateTable: (name, patch) =>
    set((state) => {
      const updatedTables = state.tables.map((t) =>
        t.name === name ? { ...t, ...patch } : t
      );
      // Auto-select a table when it first reaches "done" status
      let selectedTablesForChat = state.selectedTablesForChat;
      if (patch.status === "done" && !state.selectedTablesForChat.has(name)) {
        selectedTablesForChat = new Set(state.selectedTablesForChat);
        selectedTablesForChat.add(name);
      }
      return { tables: updatedTables, selectedTablesForChat };
    }),
  removeTable: (name) =>
    set((state) => {
      const next = new Set(state.selectedTablesForChat);
      next.delete(name);
      return {
        tables: state.tables.filter((t) => t.name !== name),
        selectedTablesForChat: next,
      };
    }),
  setTables: (tables) =>
    set((state) => {
      // When loading existing tables (all "done"), add any new ones to selection
      const next = new Set(state.selectedTablesForChat);
      for (const t of tables) {
        if (t.status === "done" || t.status === "ready") next.add(t.name);
      }
      return { tables, selectedTablesForChat: next };
    }),
  addMessage: (message) =>
    set((state) => {
      const updated = [...state.chatMessages, message];
      const MAX_MESSAGES = 50;
      return {
        chatMessages: updated.length > MAX_MESSAGES
          ? updated.slice(updated.length - MAX_MESSAGES)
          : updated,
      };
    }),
  clearMessages: () =>
    set({ chatMessages: [], analysisGeoJson: { type: "FeatureCollection", features: [] }, visualizationType: "scatter", analysisTotalCount: null }),
  setIsProcessing: (value) => set({ isProcessing: value }),
  setAnalysisGeoJson: (geojson) => set({ analysisGeoJson: geojson }),
  setVisualizationType: (type) => set({ visualizationType: type }),
  setTableGeoJson: (tableName, geojson) =>
    set((state) => ({
      tableGeoJson: { ...state.tableGeoJson, [tableName]: geojson },
    })),
  clearTableGeoJson: (tableName) =>
    set((state) => {
      const next = { ...state.tableGeoJson };
      delete next[tableName];
      return { tableGeoJson: next };
    }),
  setMapActions: (actions) => set({ mapActions: actions }),
  toggleTableForChat: (tableName) =>
    set((state) => {
      const next = new Set(state.selectedTablesForChat);
      if (next.has(tableName)) {
        next.delete(tableName);
      } else {
        next.add(tableName);
      }
      return { selectedTablesForChat: next };
    }),
  selectAllTablesForChat: () =>
    set((state) => ({
      selectedTablesForChat: new Set(
        state.tables
          .filter((t) => t.status === "done" || t.status === "ready")
          .map((t) => t.name)
      ),
    })),
  deselectAllTablesForChat: () =>
    set({ selectedTablesForChat: new Set<string>() }),
  setActiveWidget: (widget) => set({ activeWidget: widget }),
  clearActiveWidget: () => set({ activeWidget: null }),
  fetchApiConfig: async () => {
    if (!isTauri()) return;
    try {
      const raw = await invoke<string>("check_api_config");
      const config = JSON.parse(raw) as ApiConfig;
      set({ apiConfig: config });
    } catch {
      // If the command fails, treat all keys as absent to show warnings
      set({ apiConfig: { gemini: false, geocodio: false } });
    }
  },
  fetchLogPath: async () => {
    if (!isTauri()) return;
    try {
      const path = await invoke<string>("get_log_path");
      set({ logPath: path });
    } catch {
      // Non-fatal — just leave logPath as null
    }
  },
  fetchDomainConfig: async () => {
    if (!isTauri()) return;
    try {
      const raw = await invoke<string>("get_domain_pack_config");
      const config = JSON.parse(raw) as DomainPackConfig;
      set({ domainConfig: config });
    } catch {
      // Non-fatal — keep default config
    }
  },
  setBasemapId: (id) => {
    if (typeof localStorage !== "undefined") localStorage.setItem("basemapId", id);
    set({ basemapId: id });
  },
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  setAnalysisTotalCount: (count) => set({ analysisTotalCount: count }),
});

export const useAppStore = create<AppStore>(storeInitializer);
