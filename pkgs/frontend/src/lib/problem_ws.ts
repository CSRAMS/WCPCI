import type { Status } from "@/components/CaseIndicator.astro";
import confetti from "canvas-confetti";

export type WebSocketRequest =
    | {
          type: "judge";
          program: string;
          language: string;
      }
    | {
          type: "test";
          program: string;
          language: string;
          input: string;
      };

export type CaseStatus =
    | {
          status: "running";
      }
    | {
          status: "pending";
      }
    | {
          status: "passed";
          content: string | null;
      }
    | {
          status: "failed";
          content: [boolean, string];
      }
    | {
          status: "notRun";
      };

export type JobState =
    | {
          type: "judging";
          cases: CaseStatus[];
      }
    | {
          type: "testing";
          status: CaseStatus;
      };

export type WebSocketMessage =
    | {
          type: "stateUpdate";
          state: JobState;
      }
    | {
          type: "runStarted";
      }
    | {
          type: "runDenied";
          reason: string;
      }
    | {
          type: "invalid";
          error: string;
      };

function randomInRange(min: number, max: number) {
    return Math.random() * (max - min) + min;
}

export default (
    contestId: string,
    problemId: string,
    runMessageWrapper: HTMLElement,
    runMessage: HTMLElement,
    debugCaseIndicator: HTMLElement,
    testOutput: HTMLTextAreaElement,
    toggleButtons: (disabled: boolean) => void
) => {
    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    const url = `${scheme}://${window.location.host}/run/ws/${contestId}/${problemId}`;
    console.debug("Connecting to WebSocket at", url);
    const ws = new WebSocket(url);

    const stateIsComplete = (state: JobState) => {
        switch (state.type) {
            case "judging":
                return !state.cases.some((c) => c.status === "pending" || c.status === "running");
            case "testing":
                return state.status.status !== "pending" && state.status.status !== "running";
        }
    };

    const confettiForElem = (elem: Element, options: Partial<confetti.Options>) => {
        const rect = elem.getBoundingClientRect();
        const windowWidth = window.innerWidth;
        const windowHeight = window.innerHeight;
        confetti({
            ...options,
            disableForReducedMotion: true,
            origin: { x: (rect.left + rect.width / 2) / windowWidth, y: rect.top / windowHeight }
        });
    };

    const typeToStatus: Record<CaseStatus["status"], Status> = {
        failed: "error",
        passed: "success",
        notRun: "empty",
        pending: "idle",
        running: "loading"
    };

    ws.onopen = () => {
        console.debug("WebSocket connection established");
        toggleButtons(false);
    };

    ws.onmessage = (event) => {
        const message: WebSocketMessage = JSON.parse(event.data);
        console.debug("Received message", message);

        switch (message.type) {
            case "stateUpdate":
                const state = message.state as JobState;
                const complete = stateIsComplete(state);
                toggleButtons(!complete);
                switch (state.type) {
                    case "judging":
                        for (const [i, c] of state.cases.entries()) {
                            const elem = document.querySelector(
                                `[data-case-number='${i}']`
                            )! as HTMLElement;
                            const currentStatus = elem.getAttribute("data-status");
                            if (currentStatus === typeToStatus[c.status]) {
                                continue;
                            }
                            if (c.status === "passed") {
                                confettiForElem(elem, {
                                    particleCount: randomInRange(30, 35),
                                    angle: randomInRange(70, 110),
                                    startVelocity: 15,
                                    spread: 45
                                });
                            }
                            elem.setAttribute("data-status", typeToStatus[c.status]);
                        }
                        if (complete) {
                            const firstWithErr = state.cases.find((c) => c.status === "failed");
                            if (firstWithErr && firstWithErr.status === "failed") {
                                runMessageWrapper.setAttribute("data-status", "error");
                                runMessage.innerText = firstWithErr.content[1];
                            } else {
                                runMessageWrapper.setAttribute("data-status", "success");
                                runMessage.innerText = "All Tests Passed!";
                                confettiForElem(runMessage, {
                                    particleCount: 40,
                                    spread: 360,
                                    ticks: 50,
                                    gravity: 0,
                                    decay: 0.8,
                                    startVelocity: 30,
                                    colors: ["FFE400", "FFBD00", "E89400", "FFCA6C", "FDFFB8"],
                                    shapes: ["star"]
                                });
                            }
                        } else {
                            runMessageWrapper.setAttribute("data-status", "loading");
                            runMessage.innerText = "Running...";
                        }
                        break;
                    case "testing":
                        debugCaseIndicator.setAttribute(
                            "data-status",
                            typeToStatus[state.status.status]
                        );
                        switch (state.status.status) {
                            case "passed":
                                testOutput.value = state.status.content ?? "";
                                break;
                            case "failed":
                                testOutput.value = state.status.content[1] ?? "";
                                break;
                        }
                }
                break;
            case "invalid":
                console.error("Invalid message sent", message);
                toggleButtons(false);
                break;
            case "runDenied":
                runMessageWrapper.setAttribute("data-status", "error");
                runMessage.innerText = message.reason;
                toggleButtons(false);
                break;
            case "runStarted":
                toggleButtons(true);
                break;
        }
    };

    ws.onclose = () => {
        console.debug("WebSocket connection closed");
        toggleButtons(true);
        runMessageWrapper.setAttribute("data-status", "disconnected");
        runMessage.innerText = "Disconnected, please refresh the page.";
    };

    ws.onerror = (error) => {
        console.error("WebSocket error:", error);
    };

    return ws;
};
