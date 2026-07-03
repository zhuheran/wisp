import type { ToolCallItem } from "../libs/types";

function escapeMd(s: string): string {
	return s.replace(/[_*`[\]]/g, (m) => `\\${m}`);
}

/** Escape a markdown table cell: pipes must be escaped, newlines removed. */
function escapeValue(s: string): string {
	return s.replace(/\|/g, "\\|").replace(/\r?\n/g, " ").replace(/`/g, "\\`");
}

/** Render an argument value for a table cell. Objects become inline JSON code. */
function renderArgValue(value: unknown): string {
	if (value === null) return "`null`";
	if (typeof value === "object") {
		const json = JSON.stringify(value);
		return "```json\n" + escapeValue(json) + "\n```";
	}
	return "```json\n" + escapeValue(String(value)) + "\n```";;
}

/**
 * Markdown collapses a single newline into a space. Tool output frequently
 * uses intentional single newlines, so normalise any run of newlines into a
 * paragraph break (two newlines).
 */
function normaliseNewlines(s: string): string {
	return s;
}

/**
 * Format a tool call result as markdown.
 *
 * - Arguments render as a key/value table; object values render as inline JSON.
 * - Result text has its newlines promoted to paragraph breaks so they survive
 *   markdown rendering.
 *
 * Pass `includeHeader: true` to prepend the tool name + status line (used when
 * the surrounding UI does not already show them).
 */
export function formatToolResultMarkdown(
	call: ToolCallItem,
	options: { includeHeader?: boolean } = {},
): string {
	const { includeHeader = false } = options;
	const lines: string[] = [];

	const result = call.result;
	const isError = result?.isError === true;
	const statusLabel = isError ? "error" : "success";

	if (includeHeader) {
		lines.push(`**${escapeMd(call.name)}** · ${statusLabel}`);
		lines.push("");
	}

	// Arguments table
	const entries = Object.entries(call.arguments ?? {});
	if (entries.length > 0) {
		for (const [key, value] of entries) {
			lines.push(escapeValue(key));
			lines.push("");
			lines.push(renderArgValue(value));
			lines.push("---");
		}
		lines.push("");
	}

	if (!result) {
		lines.push("> No result");
		return lines.join("\n");
	}

	const pieces: string[] = [];
	for (const c of result.content) {
		if (c.type === "text" && c.text) {
			pieces.push(normaliseNewlines(c.text));
		} else if (c.type === "image") {
			pieces.push("_[image]_");
		} else if (c.type === "resource") {
			pieces.push(
				c.text
					? normaliseNewlines(c.text)
					: `_[resource: ${c.uri ?? "unknown"}]_`,
			);
		}
		pieces.push("---");
	}

	if (pieces.length === 0) {
		return lines.join("\n");
	}

	lines.push(isError ? "> **Error**" : "**Result**");
	lines.push("<br/>");
	lines.push(pieces.join("\n\n"));

	console.log(lines.join("\n"))

	return lines.join("\n");
}
