import { describe, it, expect } from "vitest";
import { createStreamingAccumulator } from "../../stores/chat";

describe("createStreamingAccumulator", () => {
  it("accumulates text and reasoning within the same message", () => {
    const acc = createStreamingAccumulator();
    expect(acc.pushText("m1", "Hello ")).toBe("Hello ");
    expect(acc.pushText("m1", "world")).toBe("Hello world");
    expect(acc.pushReasoning("m1", "think ")).toBe("think ");
    expect(acc.pushReasoning("m1", "more")).toBe("think more");
    expect(acc.text).toBe("Hello world");
    expect(acc.reasoning).toBe("think more");
  });

  it("resets both accumulators when message_id changes (tool-call round)", () => {
    const acc = createStreamingAccumulator();
    acc.pushText("m1", "round 1 text");
    acc.pushReasoning("m1", "round 1 reasoning");

    // New round => new message_id => accumulators must reset before appending.
    expect(acc.pushText("m2", "round 2 text")).toBe("round 2 text");
    expect(acc.pushReasoning("m2", "round 2 reasoning")).toBe("round 2 reasoning");

    expect(acc.text).toBe("round 2 text");
    expect(acc.reasoning).toBe("round 2 reasoning");
  });

  it("resetting on id switch also clears the other accumulator", () => {
    const acc = createStreamingAccumulator();
    acc.pushText("m1", "kept");
    acc.pushReasoning("m1", "reason");

    // Pushing reasoning for a new id should NOT carry over the prior text.
    expect(acc.pushReasoning("m2", "new reason")).toBe("new reason");
    expect(acc.text).toBe("");
  });

  it("treats same message_id across multiple pushes without resetting", () => {
    const acc = createStreamingAccumulator();
    acc.pushText("m1", "a");
    acc.pushReasoning("m1", "x");
    acc.pushText("m1", "b");
    acc.pushReasoning("m1", "y");
    expect(acc.text).toBe("ab");
    expect(acc.reasoning).toBe("xy");
  });
});
