<script lang="ts" setup>
import { onMounted, onBeforeUnmount, ref } from 'vue'

const props = defineProps<{
  auto?: boolean
  throttle?: number
  smooth?: boolean
}>()

const container = ref<HTMLElement | null>(null)
// Distance from the bottom (in px) within which we consider the user to be
// "following" the latest content and auto-scroll is acceptable.
const bottomThreshold = props.throttle && props.throttle > 0 ? props.throttle : 80
let resizeObserver: ResizeObserver | null = null

// Tracks whether the user is currently at (or near) the bottom of the
// scroll area. Updated on every scroll event. When the user scrolls up to
// read history, this becomes false and auto-scroll is suspended until they
// scroll back down.
let wasAtBottom = true

const isAtBottom = () => {
  if (!container.value) return true
  const { scrollTop, clientHeight, scrollHeight } = container.value
  return scrollHeight - scrollTop - clientHeight < bottomThreshold
}

const onScroll = () => {
  wasAtBottom = isAtBottom()
}

/**
 * Scroll to the bottom of the content.
 *
 * @param force  When true, always scroll regardless of the user's current
 *               scroll position (e.g. when the user sends a message or when
 *               a conversation is first loaded). When false (default), the
 *               scroll is only performed if the user hasn't scrolled away
 *               from the bottom, so they can freely scroll up to read
 *               history without being yanked back down.
 * @param smooth Whether to use smooth scrolling.
 */
const scrollToBottom = (force = false, smooth = props.smooth) => {
  if (!container.value) return
  if (!force && !wasAtBottom) return

  container.value.scrollTo({
    top: container.value.scrollHeight,
    behavior: smooth ? 'smooth' : 'auto'
  })
}

onMounted(() => {
  if (!container.value) return

  container.value.addEventListener('scroll', onScroll, { passive: true })
  wasAtBottom = true

  if (props.auto !== false) {
    scrollToBottom(true)
  }

  // Observe the inner content wrapper for size changes (new message bubbles,
  // streaming text updates, tool-call result expansion, etc.) and auto-scroll.
  // A ResizeObserver fires AFTER the DOM has actually been mutated, so the
  // scroll position always matches the real content height.
  //
  // The observer respects `wasAtBottom`: if the user has scrolled up, content
  // growth (e.g. streaming text) will NOT pull them back to the bottom.
  const content = container.value.firstElementChild as HTMLElement | null
  if (content && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => {
      if (props.auto !== false) {
        scrollToBottom(false)
      }
    })
    resizeObserver.observe(content)
  }
})

onBeforeUnmount(() => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  container.value?.removeEventListener('scroll', onScroll)
})

defineExpose({
  scrollToBottom
})
</script>

<template>
  <div ref="container" style="height: 100%; overflow: auto;">
    <div style="width: 100%; height: fit-content;">
      <slot />
    </div>
  </div>
</template>
