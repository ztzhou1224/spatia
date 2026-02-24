import { useState, useRef, type RefObject } from "react";
import maplibregl from "maplibre-gl";
import { TextField, Card, Box, Text, Flex } from "@radix-ui/themes";
import { safeInvoke } from "../lib/tauri";
import { useFocusGuard } from "../lib/useFocusGuard";
import { useWidgetStore } from "../lib/widgetStore";

type OvertureSearchResult = {
  id?: string;
  label: string;
};

type OvertureGeocodeResult = {
  id?: string;
  label: string;
  lat?: number;
  lon?: number;
};

interface Props {
  mapRef: RefObject<maplibregl.Map | null>;
  dbPath: string;
  tableName: string;
}

function quoteArg(value: string): string {
  return `"${value.replace(/"/g, " ").trim()}"`;
}

export function SearchWidget({ mapRef, dbPath, tableName }: Props) {
  const appFocusedWidgetId = useWidgetStore(
    (state) => state.appFocusedWidgetId,
  );
  const focusGuard = useFocusGuard({
    id: "search-widget",
    label: "Search Widget",
    kind: "search",
  });
  const [query, setQuery] = useState("");
  const [suggestions, setSuggestions] = useState<OvertureSearchResult[]>([]);
  const [open, setOpen] = useState(false);
  const markerRef = useRef<maplibregl.Marker | null>(null);

  async function runOvertureSearch(
    text: string,
  ): Promise<OvertureSearchResult[]> {
    const command = `overture_search ${quoteArg(dbPath)} ${quoteArg(tableName)} ${quoteArg(text)} 10`;
    const raw = await safeInvoke<string>(
      "execute_engine_command",
      { command },
      "[]",
    );
    if (!raw) return [];
    try {
      return JSON.parse(raw) as OvertureSearchResult[];
    } catch {
      return [];
    }
  }

  async function geocodeSelection(
    label: string,
  ): Promise<OvertureGeocodeResult | null> {
    const command = `overture_geocode ${quoteArg(dbPath)} ${quoteArg(tableName)} ${quoteArg(label)} 1`;
    const raw = await safeInvoke<string>("execute_engine_command", {
      command,
    });
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw) as OvertureGeocodeResult[];
      return parsed[0] ?? null;
    } catch {
      return null;
    }
  }

  async function handleChange(e: React.ChangeEvent<HTMLInputElement>) {
    const val = e.target.value;
    setQuery(val);
    if (val.trim().length < 2) {
      setSuggestions([]);
      setOpen(false);
      return;
    }
    const matches = await runOvertureSearch(val);
    setSuggestions(matches);
    setOpen(matches.length > 0);
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
      setOpen(false);
    }
  }

  async function selectResult(r: OvertureSearchResult) {
    setQuery(r.label);
    setSuggestions([]);
    setOpen(false);

    const resolved = await geocodeSelection(r.label);
    if (!resolved || resolved.lat === undefined || resolved.lon === undefined) {
      return;
    }

    const map = mapRef.current;
    if (!map) return;
    map.flyTo({
      center: [resolved.lon, resolved.lat],
      zoom: 13,
      duration: 1000,
    });
    if (markerRef.current) {
      markerRef.current.setLngLat([resolved.lon, resolved.lat]);
    } else {
      markerRef.current = new maplibregl.Marker()
        .setLngLat([resolved.lon, resolved.lat])
        .addTo(map);
    }
  }

  return (
    <Box
      className={`search-widget ${appFocusedWidgetId === "search-widget" ? "widget-focus-ring" : ""}`}
      {...focusGuard}
    >
      <TextField.Root
        value={query}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onBlur={() => setOpen(false)}
        placeholder="🔍 Search places…"
        size="2"
        variant="soft"
      />
      {open && suggestions.length > 0 && (
        <Card className="search-suggestions" size="1">
          <Flex direction="column">
            {suggestions.map((r) => (
              <Box
                key={`${r.id ?? r.label}-${r.label}`}
                px="3"
                py="2"
                className="search-suggestion-item"
                style={{ cursor: "pointer", borderRadius: "var(--radius-2)" }}
                onMouseDown={(e) => {
                  e.preventDefault(); // prevent input blur before click fires
                  selectResult(r);
                }}
              >
                <Text size="2" as="div">
                  {r.label}
                </Text>
              </Box>
            ))}
          </Flex>
        </Card>
      )}
    </Box>
  );
}
