export const makeCountdown = (
    startTime: Date,
    onTick?: (formatted: string, totalSeconds: number) => void,
    onEnd?: () => void,
    onPassed?: () => void
) => {
    const start = startTime;
    const now = new Date();
    if (start > now) {
        const updateTimer = () => {
            const now = new Date();
            const diff = start.getTime() - now.getTime();
            if (diff <= 0) {
                clearInterval(interval);
                onEnd?.();
            } else {
                const totalSeconds = Math.floor(diff / 1000);
                let days = Math.floor(diff / (1000 * 60 * 60 * 24)).toString();
                days = days === "0" ? "" : `${days}d `;
                let hours = Math.floor(
                    (diff % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60)
                ).toString();
                hours = hours === "0" ? "" : `${hours}h `;
                let minutes = Math.floor((diff % (1000 * 60 * 60)) / (1000 * 60)).toString();
                minutes = minutes === "0" ? "" : `${minutes}m `;
                const seconds = Math.floor((diff % (1000 * 60)) / 1000);
                const text = `${days}${hours}${minutes}${seconds}s`;
                onTick?.(text, totalSeconds);
            }
        };
        updateTimer();
        const interval = setInterval(updateTimer, 1000);
    } else {
        onPassed?.();
    }
};
