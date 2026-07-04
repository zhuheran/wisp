import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkMath from 'remark-math'
import remarkGfm from 'remark-gfm'
import remarkHtml from 'remark-html'
import rehypeSanitize from 'rehype-sanitize'
import rehypeParse from 'rehype-parse'
import rehypeStringify from 'rehype-stringify'
import { h } from 'vue'
import { NCode, NH1, NH2, NH3, NH4, NH5, NH6, NA, NBlockquote, NUl, NOl, NLi, NText, NTable, NDivider } from 'naive-ui'
import CodeMermaidRenderer from '../components/CodeMermaidRenderer.vue'
import KatexRenderer from '../components/KatexRenderer.vue'
import { toVNode } from '../libs/to-vnode'
import { Code, InlineCode, Root, Html, Heading, List, Paragraph } from 'mdast'


function createMarkdownProcessor() {
	return unified()
		.use(remarkParse)
		.use(remarkHtml, { allowDangerousHtml: true })
		.use(remarkMath, { singleDollarTextMath: true })
		.use(remarkGfm)
}

function createSanitizer() {
	return unified()
		.use(rehypeParse, { fragment: true })
		.use(rehypeSanitize)
		.use(rehypeStringify)
}

export function useVNodeRenderer() {
	const processor = createMarkdownProcessor()
	const sanitizer = createSanitizer()

	const sanitize = (html: string) => sanitizer.processSync(html).toString()

	const getInlineMathComponent = (node: InlineCode) => h(KatexRenderer, {
		katexOptions: {
			displayMode: false,
			output: 'mathml'
		},
		value: node.value,
		style: "font-size: 1.15em; padding: 2px; overflow: auto;",
		inline: true
	})

	const getMathComponent = (node: InlineCode) => h(KatexRenderer, {
		katexOptions: {
			displayMode: true,
			output: 'mathml'
		},
		value: node.value,
		style: "font-size: 1.2em; padding: 4px; overflow: auto;",
		inline: false
	})


	return async (text: string, enableLastMermaid: boolean = false, mermaidCallback?: (success: boolean) => void, parseCallback?: (containMermaid: boolean) => void) => {
		try {
			const tree = processor.parse(text)

			if (text.trim().startsWith("**code")) {
				console.log(text)
				console.log(tree)
			}

			const disableLastMermaidRc = (node: Root) => {
				if (!node) return
				if (!node.children) return;
				const last = node.children[node.children.length - 1]
				if (!last) return;

				if (last.type === 'code') {
					if (last.lang === 'mermaid-live') {
						last.lang = 'mermaid-generating'
					}
				}
				else {
					disableLastMermaidRc(node.children[node.children.length - 1] as unknown as Root)
				}
			}

			const containMermaidLive = (node: Root) => {
				if (!node) return false
				if (!node.children) return false;
				for (const child of node.children) {
					if (child.type === 'code') {
						if (child.lang === 'mermaid-live') {
							return true
						}
					}
					else {
						if (containMermaidLive(child as unknown as Root)) {
							return true
						}
					}
				}
				return false
			}

			if (parseCallback) parseCallback(containMermaidLive(tree))

			if (!enableLastMermaid) disableLastMermaidRc(tree)
			const vNode = toVNode(tree, {
				components: {
					inlineMath: getInlineMathComponent,
					math: getMathComponent,
					code: (node: Code) => h(CodeMermaidRenderer, { code: node.value ?? "", language: node.lang ?? "", onReady: mermaidCallback }),
					inlineCode: (node: InlineCode) => h(NCode, { code: node.value ?? "", inline: true, wordWrap: true, style: { transition: 'none !important' } }),
					html: (node: Html) => h('div', { innerHTML: sanitize(node.value) }),

					heading: (node: Heading) => {
						const depth = node.depth
						const headings = [NH1, NH2, NH3, NH4, NH5, NH6]
						return h(headings[depth - 1], { alignText: true })
					},
					link: NA,
					blockquote: NBlockquote,
					list: (node: List) => node.ordered ? NOl : NUl,
					listItem: NLi,
					paragraph: (node: Paragraph) => h('div', { style: "display: block" }, h(NText, { text: node.data, depth: 2 })),
					strong: h(NText, { strong: true, style: "filter: contrast(1.6)" }),
					emphasis: h(NText, { italic: true }),
					table: h(NTable, { bordered: true, singleLine: false }),
					thematicBreak: NDivider,
					break: h('br')
				}
			})
			return vNode
		}
		catch (error) {
			console.error(error)
			return null
		}
	}
}
