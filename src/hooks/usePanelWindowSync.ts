import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { tauriApi } from "../services/tauriApi";

/** Mirrors panel visibility and user-resized dimensions to the native window. */
export function usePanelWindowSync(isPanelOpen: boolean) {
  useEffect(() => {
    tauriApi.togglePanel(isPanelOpen).catch((error) => {
      console.error("Failed to toggle panel:", error);
    });
  }, [isPanelOpen]);

  useEffect(() => {
    if (!isPanelOpen) return;

    let cancelled = false;
    let debounceTimer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;

    Promise.resolve()
      .then(() => getCurrentWindow().onResized(({ payload: size }) => {
        if (cancelled) return;
        if (debounceTimer) clearTimeout(debounceTimer);

        debounceTimer = setTimeout(() => {
          if (cancelled) return;
          getCurrentWindow()
            .isMaximized()
            .then((maximized) => {
              if (!cancelled && !maximized) {
                return tauriApi.savePanelSize(size.width, size.height);
              }
              return undefined;
            })
            .catch(console.error);
        }, 500);
      }))
      .then((unsubscribe) => {
        if (cancelled) {
          unsubscribe();
          return;
        }
        unlisten = unsubscribe;
      })
      .catch(console.error);

    return () => {
      cancelled = true;
      if (debounceTimer) clearTimeout(debounceTimer);
      unlisten?.();
    };
  }, [isPanelOpen]);
}
