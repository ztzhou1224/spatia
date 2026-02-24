import { useCallback, useEffect } from "react";
import { type Widget, useWidgetStore } from "./widgetStore";

type FocusGuardConfig = {
  id: string;
  label: string;
  kind: Widget["kind"];
};

export function useFocusGuard(config: FocusGuardConfig) {
  const registerWidget = useWidgetStore((state) => state.registerWidget);
  const unregisterWidget = useWidgetStore((state) => state.unregisterWidget);
  const setAppFocus = useWidgetStore((state) => state.setAppFocus);

  useEffect(() => {
    registerWidget({
      id: config.id,
      label: config.label,
      kind: config.kind,
      metadata: {},
    });

    return () => {
      unregisterWidget(config.id);
    };
  }, [config.id, config.kind, config.label, registerWidget, unregisterWidget]);

  const onPointerDownCapture = useCallback(() => {
    setAppFocus(config.id);
  }, [config.id, setAppFocus]);

  return { onPointerDownCapture };
}
