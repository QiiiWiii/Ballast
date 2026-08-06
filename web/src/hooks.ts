import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

export function useEventStream(): boolean {
  const queryClient = useQueryClient();
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    let socket: WebSocket | undefined;
    let retry: number | undefined;
    let stopped = false;
    let attempt = 0;

    const connect = () => {
      const protocol = location.protocol === "https:" ? "wss:" : "ws:";
      socket = new WebSocket(`${protocol}//${location.host}/api/v1/ws`);
      socket.onopen = () => { attempt = 0; setConnected(true); };
      socket.onmessage = (event) => {
        const message = JSON.parse(String(event.data)) as { type?: string };
        if (message.type === "execution_event") {
          void queryClient.invalidateQueries({ queryKey: ["tasks"] });
          void queryClient.invalidateQueries({ queryKey: ["task"] });
          void queryClient.invalidateQueries({ queryKey: ["slices"] });
        }
        if (message.type === "market_health") {
          void queryClient.invalidateQueries({ queryKey: ["exchanges"] });
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
      socket?.close();
    };
  }, [queryClient]);

  return connected;
}
