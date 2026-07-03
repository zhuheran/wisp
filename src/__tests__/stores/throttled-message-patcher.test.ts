import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createThrottledMessagePatcher } from "../../stores/chat";

describe("createThrottledMessagePatcher", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("does not apply synchronously; waits for the interval", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "a" });
		expect(apply).not.toHaveBeenCalled();

		vi.advanceTimersByTime(299);
		expect(apply).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(apply).toHaveBeenCalledTimes(1);
		expect(apply).toHaveBeenCalledWith("m1", { text: "a" });
	});

	it("coalesces multiple patches within the interval into one apply", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "a" });
		p.schedule("m1", { text: "b" });
		p.schedule("m1", { text: "c" });

		vi.advanceTimersByTime(300);

		expect(apply).toHaveBeenCalledTimes(1);
		expect(apply).toHaveBeenCalledWith("m1", { text: "c" });
	});

	it("merges concurrent text and reasoning patches", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "hello" });
		p.schedule("m1", { reasoning: "think" });

		vi.advanceTimersByTime(300);

		expect(apply).toHaveBeenCalledTimes(1);
		expect(apply).toHaveBeenCalledWith("m1", {
			text: "hello",
			reasoning: "think",
		});
	});

	it("flushes pending immediately when message_id changes", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "round 1" });
		expect(apply).not.toHaveBeenCalled();

		p.schedule("m2", { text: "round 2" });

		// Previous round flushed synchronously on mid switch.
		expect(apply).toHaveBeenCalledWith("m1", { text: "round 1" });

		vi.advanceTimersByTime(300);
		expect(apply).toHaveBeenCalledWith("m2", { text: "round 2" });
		expect(apply).toHaveBeenCalledTimes(2);
	});

	it("flush() forces an immediate apply", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "x" });
		expect(apply).not.toHaveBeenCalled();

		p.flush();
		expect(apply).toHaveBeenCalledWith("m1", { text: "x" });
	});

	it("flush() with nothing pending is a no-op", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);
		p.flush();
		expect(apply).not.toHaveBeenCalled();
	});

	it("does not apply again after flush until next schedule", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "a" });
		vi.advanceTimersByTime(300);
		expect(apply).toHaveBeenCalledTimes(1);

		// No further schedules — advancing time must not re-apply.
		vi.advanceTimersByTime(1000);
		expect(apply).toHaveBeenCalledTimes(1);
	});

	it("respects the interval between successive flushes", () => {
		const apply = vi.fn();
		const p = createThrottledMessagePatcher(apply, 300);

		p.schedule("m1", { text: "a" });
		vi.advanceTimersByTime(300);
		expect(apply).toHaveBeenLastCalledWith("m1", { text: "a" });

		p.schedule("m1", { text: "b" });
		vi.advanceTimersByTime(299);
		expect(apply).toHaveBeenCalledTimes(1); // not yet
		vi.advanceTimersByTime(1);
		expect(apply).toHaveBeenLastCalledWith("m1", { text: "b" });
		expect(apply).toHaveBeenCalledTimes(2);
	});
});
