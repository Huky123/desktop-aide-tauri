import { useEffect, useRef, useLayoutEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

type EventCallback<T = unknown> = (payload: T) => void;

export function useTauriEvent<T = unknown>(
  eventName: string,
  callback: EventCallback<T>,
) {
  const callbackRef = useRef<EventCallback<T>>(callback);
  useLayoutEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;

    const setup = async () => {
      try {
        const fn = await listen<T>(eventName, (event) => {
          callbackRef.current(event.payload);
        });
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      } catch {
        console.log(
          `[useTauriEvent] 事件 "${eventName}" 仅在 Tauri 环境中可用`,
        );
      }
    };

    setup();

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [eventName]);
}
