# Widget Focus System – Design Draft

## Overview

This document outlines a custom widget-and-focus system for Spatia. The goals are:

1. **Widgets** – every interactive surface (map, search bar, layer panel, chat, table view …) is a first-class *widget* with a shared interface and a metadata bag.
2. **Map-as-widget** – the map's runtime state (center, zoom, bearing, active popups, selected features, visible layers) lives as widget metadata alongside all other widgets.
3. **AI context** – when the user sends a message, the AI receives the currently-focused widget's metadata as extra context so it can answer questions like "what am I looking at right now?".
4. **Custom focus manager** – because the browser's native `focus`/`blur` events steal focus from the map (or any non-input widget) the moment the user clicks the chat textarea, we maintain our own application-level focus state that is independent of DOM focus.

---

## 1. Widget Interface

Every widget registers itself with the **Widget Registry** using this shape:

```ts
// src/lib/widgets/types.ts

export type WidgetType =
  | "map"
  | "search"
  | "layer-panel"
  | "chat"
  | "data-table"
  | "schema-panel";

export interface WidgetMetadata {
  // --- map widget ---
  center?: [number, number];   // [lng, lat]
  zoom?: number;
  bearing?: number;
  pitch?: number;
  activePopups?: PopupInfo[];
  selectedFeatures?: GeoFeatureSummary[];
  visibleLayers?: string[];

  // --- search widget ---
  query?: string;
  results?: SearchResult[];

  // --- table/schema widget ---
  tableName?: string;
  visibleColumns?: string[];
  rowCount?: number;

  // --- any widget can carry free-form context ---
  [key: string]: unknown;
}

export interface PopupInfo {
  id: string;
  lngLat: [number, number];
  featureId?: string;
  title?: string;
}

export interface GeoFeatureSummary {
  id: string;
  type: string;
  properties: Record<string, string | number | boolean | null>;
}

export interface SearchResult {
  id: string;
  name: string;
  lngLat?: [number, number];
}

export interface Widget {
  id: string;
  type: WidgetType;
  label: string;                  // human-readable, used in AI prompts
  metadata: WidgetMetadata;
  isFocused: boolean;             // application-level focus, NOT DOM focus
}
```

---

## 2. Widget Registry (Zustand Store)

A single Zustand store holds all registered widgets and the id of the one that currently has application focus:

```ts
// src/lib/widgets/useWidgetStore.ts  (pseudocode)

interface WidgetStore {
  widgets: Record<string, Widget>;

  // id of the widget that "owns" the user's attention
  // this is NOT reset when the chatbox gains browser focus
  focusedWidgetId: string | null;

  // id of the widget that had focus just before the chat widget gained
  // browser focus; used by the AI to know what was "in view"
  lastNonChatFocusedWidgetId: string | null;

  // actions
  registerWidget:   (widget: Omit<Widget, "isFocused">) => void;
  unregisterWidget: (id: string) => void;
  setAppFocus:      (id: string) => void;   // explicit app-level focus change
  updateMetadata:   (id: string, patch: Partial<WidgetMetadata>) => void;
}
```

Key invariants:
- `focusedWidgetId` changes only when the user **explicitly interacts** with a widget surface (click, touch, keyboard shortcut). It is never cleared just because the chat textarea gains browser focus.
- When the **chat widget** gains browser focus, the store saves the current `focusedWidgetId` into `lastNonChatFocusedWidgetId` before updating.

---

## 3. Custom Focus Manager

### 3.1 Why Not Browser Focus

The browser's native active-element mechanism is scoped to DOM nodes that receive keyboard input. When the user clicks the chat textarea:

- the map container loses `document.activeElement`
- MapLibre stops receiving keyboard shortcuts
- any focus-ring CSS on map panels clears

This means standard `:focus` and `onFocus` / `onBlur` event handlers cannot reliably track "which GIS widget was the user working with before they asked a question."

### 3.2 Application Focus Rules

| User action | DOM focus changes to | App focus changes to |
|---|---|---|
| Click map | map container | `map` widget |
| Click search bar | search input | `search` widget |
| Click layer panel toggle | layer button | `layer-panel` widget |
| Click chat textarea | chat input | stays on previous widget; `lastNonChatFocusedWidgetId` = previous |
| Submit chat message | (no DOM change) | (no change) |
| Click outside all widgets | browser body | `null` (no widget focused) |

The distinction above decouples "which widget has keyboard events" from "which widget is the user's active GIS context."

### 3.3 FocusGuard Hook

Each widget wraps its root element with a `useFocusGuard` hook:

```ts
// src/lib/widgets/useFocusGuard.ts  (pseudocode)

function useFocusGuard(widgetId: string, options?: { isChatInput?: boolean }) {
  const setAppFocus = useWidgetStore(s => s.setAppFocus);

  const onPointerDown = () => {
    if (!options?.isChatInput) {
      // normal widget: claim app focus immediately
      setAppFocus(widgetId);
    }
    // chat input: focus manager handles lastNonChatFocusedWidgetId
    // inside setAppFocus when type === "chat"
  };

  return { onPointerDown };
}
```

Because `onPointerDown` fires before the browser reroutes keyboard focus, the store update is atomic with the user gesture.

---

## 4. Map Widget – Metadata Lifecycle

The map widget syncs its MapLibre runtime state into the widget store whenever the map changes:

```
MapLibre events → updateMetadata("map", { center, zoom, bearing, pitch })
popup open/close → updateMetadata("map", { activePopups: [...] })
feature click    → updateMetadata("map", { selectedFeatures: [...] })
layer toggle     → updateMetadata("map", { visibleLayers: [...] })
```

This keeps the store as the single source of truth for map context. No direct reads of the MapLibre instance are needed outside the map component.

---

## 5. AI Context Injection

When the user submits a chat message, the AI client assembles a context block from the widget store:

```ts
// src/lib/ai/buildContext.ts  (pseudocode)

function buildAIContext(store: WidgetStore): string {
  const contextWidgetId =
    store.lastNonChatFocusedWidgetId ?? store.focusedWidgetId;

  if (!contextWidgetId) return "";

  const widget = store.widgets[contextWidgetId];
  if (!widget) return "";

  const lines: string[] = [
    `The user is currently focused on: ${widget.label} (${widget.type}).`,
  ];

  if (widget.type === "map") {
    const m = widget.metadata;
    if (m.center)  lines.push(`Map center: [${m.center[0].toFixed(4)}, ${m.center[1].toFixed(4)}]`);
    if (m.zoom)    lines.push(`Zoom level: ${m.zoom.toFixed(1)}`);
    if (m.bearing) lines.push(`Bearing: ${m.bearing.toFixed(1)}°`);
    if (m.visibleLayers?.length)
      lines.push(`Visible layers: ${m.visibleLayers.join(", ")}`);
    if (m.activePopups?.length)
      lines.push(`Open popups: ${m.activePopups.map(p => p.title ?? p.id).join(", ")}`);
    if (m.selectedFeatures?.length)
      lines.push(`Selected features: ${m.selectedFeatures.map(f => f.id).join(", ")}`);
  }

  if (widget.type === "search") {
    if (widget.metadata.query)
      lines.push(`Search query: "${widget.metadata.query}"`);
  }

  if (widget.type === "data-table" || widget.type === "schema-panel") {
    if (widget.metadata.tableName)
      lines.push(`Active table: ${widget.metadata.tableName}`);
  }

  return lines.join("\n");
}
```

This string is prepended to the system prompt (or inserted as an assistant context message) before the user's question is sent to the model.

---

## 6. Chat Widget – Focus Coexistence

The chat input is also a widget but it needs special treatment:

- It claims **DOM focus** (needed for typing) but does not steal **app focus** from the widget the user was previously interacting with.
- The chat component renders a subtle "context pill" below the input showing the currently referenced widget (e.g. "📍 Using context from: Map – zoom 13, downtown SF").
- Clicking that pill opens a popover listing all widgets and allowing the user to pin context to a different one.

```
┌─────────────────────────────────────────────────┐
│  Ask anything about your map...          [Send]  │
│  ┌─ Context ──────────────────────────────────┐  │
│  │ 📍 Map · zoom 13 · center (−122.4, 37.7)   │  │
│  └────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

---

## 7. Component Hierarchy (Planned)

```
<App>
  <WidgetStoreProvider>          ← Zustand store root
    <Sidebar />
    <WorkspaceArea>
      <MapWidget id="map" />     ← registers, syncs metadata, useFocusGuard
      <SearchWidget id="search" />
      <LayerPanel id="layer-panel" />
      <DataTable id="data-table" />
    </WorkspaceArea>
    <ChatPanel id="chat" />      ← useFocusGuard with isChatInput=true
  </WidgetStoreProvider>
</App>
```

---

## 8. State Shape Summary

```
WidgetStore {
  focusedWidgetId: "map" | "search" | "layer-panel" | "chat" | ... | null
  lastNonChatFocusedWidgetId: string | null
  widgets: {
    "map": {
      id: "map",
      type: "map",
      label: "Map",
      isFocused: true,
      metadata: {
        center: [-122.4194, 37.7749],
        zoom: 13,
        bearing: 0,
        pitch: 0,
        visibleLayers: ["places", "roads"],
        activePopups: [],
        selectedFeatures: []
      }
    },
    "search": { ... },
    "layer-panel": { ... },
    "chat": { ... }
  }
}
```

---

## 9. Open Questions / Decisions Needed

| # | Question | Options |
|---|---|---|
| 1 | Should `focusedWidgetId` be persisted across sessions? | Session-only (simpler) vs localStorage (remembers last view) |
| 2 | Should multiple widgets be co-focused (e.g. map + table)? | Single-focus (simpler) vs multi-focus with priority order |
| 3 | How is AI context scoped when no widget is focused? | Send no context vs send all widget metadata vs send map always |
| 4 | Should non-chat widgets also receive keyboard shortcuts when chat has DOM focus? | Yes, via global `keydown` listener that checks app focus vs No |
| 5 | Widget metadata versioning: how to handle schema changes across sessions? | Ignore stale keys vs migration helpers |

---

## 10. Implementation Phases (Suggested)

| Phase | Deliverable |
|---|---|
| 6.1 | `Widget`, `WidgetMetadata`, `WidgetStore` types + Zustand store with `registerWidget`, `setAppFocus`, `updateMetadata` |
| 6.2 | `useFocusGuard` hook; apply to existing `MapPage`, `SearchWidget` |
| 6.3 | Map metadata sync: `move`, `zoom`, `rotate`, popup, and feature-click events → `updateMetadata` |
| 6.4 | Chat context pill component using `lastNonChatFocusedWidgetId` |
| 6.5 | `buildAIContext` helper + wire into chat submit handler |
| 6.6 | Visual focus indicator on widget borders (app focus ring, not browser ring) |
