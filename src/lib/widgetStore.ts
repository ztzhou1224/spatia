import { create } from "zustand";

export type WidgetMetadata = {
  center?: [number, number];
  zoom?: number;
  bearing?: number;
  pitch?: number;
  activePopups?: number;
  selectedFeatures?: string[];
  visibleLayers?: string[];
};

export type Widget = {
  id: string;
  label: string;
  kind: "map" | "search" | "chat" | "unknown";
  metadata: WidgetMetadata;
};

type WidgetStore = {
  widgets: Record<string, Widget>;
  appFocusedWidgetId: string | null;
  lastNonChatFocusedWidgetId: string | null;
  registerWidget: (
    widget: Omit<Widget, "metadata"> & { metadata?: WidgetMetadata },
  ) => void;
  unregisterWidget: (widgetId: string) => void;
  setAppFocus: (widgetId: string | null) => void;
  updateMetadata: (widgetId: string, metadata: Partial<WidgetMetadata>) => void;
};

export const useWidgetStore = create<WidgetStore>((set) => ({
  widgets: {},
  appFocusedWidgetId: null,
  lastNonChatFocusedWidgetId: null,
  registerWidget: (widget) =>
    set((state) => ({
      widgets: {
        ...state.widgets,
        [widget.id]: {
          ...widget,
          metadata: widget.metadata ?? {},
        },
      },
    })),
  unregisterWidget: (widgetId) =>
    set((state) => {
      const { [widgetId]: _removed, ...rest } = state.widgets;
      return {
        widgets: rest,
        appFocusedWidgetId:
          state.appFocusedWidgetId === widgetId
            ? null
            : state.appFocusedWidgetId,
        lastNonChatFocusedWidgetId:
          state.lastNonChatFocusedWidgetId === widgetId
            ? null
            : state.lastNonChatFocusedWidgetId,
      };
    }),
  setAppFocus: (widgetId) =>
    set((state) => {
      const focusedWidget = widgetId ? state.widgets[widgetId] : undefined;
      return {
        appFocusedWidgetId: widgetId,
        lastNonChatFocusedWidgetId:
          focusedWidget && focusedWidget.kind !== "chat"
            ? widgetId
            : state.lastNonChatFocusedWidgetId,
      };
    }),
  updateMetadata: (widgetId, metadata) =>
    set((state) => {
      const widget = state.widgets[widgetId];
      if (!widget) {
        return state;
      }

      return {
        widgets: {
          ...state.widgets,
          [widgetId]: {
            ...widget,
            metadata: {
              ...widget.metadata,
              ...metadata,
            },
          },
        },
      };
    }),
}));
