export type ScoreEntry = {
    id: number;
    score: number;
    time_taken: number;
    num_wrong: number;
};

export type Message =
    | {
          type: "fullRefresh";
      }
    | {
          type: "unComplete";
          participantId: number;
          problemId: number;
      }
    | {
          type: "completion";
          participantId: number;
          score: ScoreEntry;
      }
    | {
          type: "completedFirst";
          participantId: number;
          problemId: number;
          isFirst: boolean;
      }
    | {
          type: "reOrder";
          participantMap: Record<number, [number, number]>;
      };

export default (
    contestId: number,
    onMsg: (msg: Message) => void,
    onClose?: () => void,
    onOpen?: () => void
) => {
    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(
        `${scheme}://${window.location.host}/contests/${contestId}/leaderboard/ws`
    );
    ws.onopen = () => {
        console.debug("Connected to leaderboard websocket");
        onOpen?.();
    };
    ws.onmessage = (event) => {
        const message = JSON.parse(event.data) as Message;
        onMsg(message);
    };
    ws.onerror = (error) => {
        console.error("Error in leaderboard websocket", error);
    };
    ws.onclose = () => {
        console.debug("Disconnected from leaderboard websocket");
        onClose?.();
    };
    return ws;
};
