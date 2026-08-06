import { createContext, useContext, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

export const EventStreamStatusContext = createContext(false);

export function useEventStreamStatus(): boolean {
  return useContext(EventStreamStatusContext);
}

export function useEventStream(): boolean {
  const queryClient = useQueryClient();
  const [connected, setConnected] = useState(false);
  const lastSequence = useRef<number | undefined>(undefined);

  useEffect(() => {
    let socket: WebSocket | undefined;
    let retry: number | undefined;
    let stopped = false;
    let attempt = 0;
    let refreshTimer: number | undefined;

    const persistSequence = (sequence: number) => {
      lastSequence.current = sequence;
      sessionStorage.setItem("ballast-event-sequence", String(sequence));
    };

    const scheduleExecutionRefresh = () => {
      if (refreshTimer !== undefined) return;
      refreshTimer = window.setTimeout(() => {
        refreshTimer = undefined;
        void Promise.all([
          queryClient.invalidateQueries({ queryKey: ["tasks"] }),
          queryClient.invalidateQueries({ queryKey: ["task"] }),
          queryClient.invalidateQueries({ queryKey: ["slices"] }),
          queryClient.invalidateQueries({ queryKey: ["events"] }),
          queryClient.invalidateQueries({ queryKey: ["dashboard"] }),
          queryClient.invalidateQueries({ queryKey: ["analytics"] }),
        ]);
      }, 120);
    };

    const connect = () => {
      const protocol = location.protocol === "https:" ? "wss:" : "ws:";
      if (lastSequence.current === undefined) {
        const rawStored = sessionStorage.getItem("ballast-event-sequence");
        const stored = rawStored === null ? Number.NaN : Number(rawStored);
        if (Number.isSafeInteger(stored) && stored >= 0) lastSequence.current = stored;
      }
      const cursor = lastSequence.current === undefined ? "" : `?after_sequence=${lastSequence.current}`;
      socket = new WebSocket(`${protocol}//${location.host}/api/v1/ws${cursor}`);
      socket.onopen = () => { attempt = 0; setConnected(true); };
      socket.onmessage = (event) => {
        const message = JSON.parse(String(event.data)) as { type?: string; data?: { sequence?: number; after_sequence?: number } };
        if (message.type === "stream_ready" && Number.isSafeInteger(message.data?.after_sequence)) {
          persistSequence(message.data!.after_sequence!);
        }
        if (message.type === "execution_event") {
          if (Number.isSafeInteger(message.data?.sequence)) persistSequence(message.data!.sequence!);
          scheduleExecutionRefresh();
        }
      };
      socket.onclose = () => {
        setConnected(false);
        if (!stopped) {
          attempt += 1;
          retry = window.setTimeout(connect, Math.min(1_000 * 2 ** attempt, 15_000));
        }
      };
    };
    connect();
    return () => {
      stopped = true;
      if (retry !== undefined) window.clearTimeout(retry);
      if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
      socket?.close();
    };
  }, [queryClient]);

  return connected;
}
