import { describe, it, expect } from "vitest";
import { formatToolResultMarkdown } from "../../utils/toolResult";
import type { ToolCallItem } from "../../libs/types";

function call(over: Partial<ToolCallItem> = {}): ToolCallItem {
  return {
    id: "c1",
    name: "get_weather",
    arguments: { location: "Hangzhou" },
    ...over,
  };
}

describe("formatToolResultMarkdown", () => {
  it("renders arguments as a key/value table", () => {
    const out = formatToolResultMarkdown(
      call({
        result: {
          isError: false,
          content: [{ type: "text", text: "Sunny, 28°C" }],
        },
      }),
    );
    expect(out).toContain("| 参数 | 值 |");
    expect(out).toContain("| --- | --- |");
    expect(out).toContain("| location | Hangzhou |");
    expect(out).toContain("**Result**");
    expect(out).toContain("Sunny, 28°C");
    // No header by default.
    expect(out).not.toMatch(/🧰|✅|❌/);
  });

  it("renders object argument values as inline JSON", () => {
    const out = formatToolResultMarkdown(
      call({
        arguments: { options: { unit: "metric", count: 3 }, q: "weather" },
        result: { isError: false, content: [{ type: "text", text: "ok" }] },
      }),
    );
    expect(out).toContain('`{"unit":"metric","count":3}`');
    expect(out).toContain("| q | weather |");
  });

  it("escapes pipes inside table cells", () => {
    const out = formatToolResultMarkdown(
      call({
        arguments: { path: "a|b|c" },
        result: { isError: false, content: [{ type: "text", text: "ok" }] },
      }),
    );
    expect(out).toContain("| path | a\\|b\\|c |");
  });

  it("promotes single newlines in result text to paragraph breaks", () => {
    const out = formatToolResultMarkdown(
      call({
        result: {
          isError: false,
          content: [{ type: "text", text: "line one\nline two\nline three" }],
        },
      }),
    );
    expect(out).toContain("line one\n\nline two\n\nline three");
    expect(out).not.toContain("line one\nline two");
  });

  it("collapses existing multi-newline runs into a single paragraph break", () => {
    const out = formatToolResultMarkdown(
      call({
        result: {
          isError: false,
          content: [{ type: "text", text: "p1\n\n\n\np2" }],
        },
      }),
    );
    expect(out).toContain("p1\n\np2");
  });

  it("renders the header line without emoji when requested", () => {
    const out = formatToolResultMarkdown(
      call({
        result: {
          isError: true,
          content: [{ type: "text", text: "boom" }],
        },
      }),
      { includeHeader: true },
    );
    expect(out).toContain("**get\\_weather** · error");
    expect(out).not.toMatch(/🧰|✅|❌/);
    expect(out).toContain("> **Error**");
    expect(out).toContain("boom");
  });

  it("renders missing result as no-result blockquote and still shows args table", () => {
    const out = formatToolResultMarkdown(call({ result: undefined }));
    expect(out).toContain("> No result");
    expect(out).toContain("| location | Hangzhou |");
  });

  it("renders image and resource content as italic placeholders", () => {
    const out = formatToolResultMarkdown(
      call({
        result: {
          isError: false,
          content: [
            { type: "text", text: "see" },
            { type: "image", data: "x", mimeType: "image/png" },
            { type: "resource", uri: "file:///a" },
          ],
        },
      }),
    );
    expect(out).toContain("_[image]_");
    expect(out).toContain("_[resource: file:///a]_");
  });

  it("omits the arguments table when there are no arguments", () => {
    const out = formatToolResultMarkdown(
      call({
        arguments: {},
        result: { isError: false, content: [{ type: "text", text: "ok" }] },
      }),
    );
    expect(out).not.toContain("| 参数 |");
    expect(out).toContain("**Result**");
  });
});
