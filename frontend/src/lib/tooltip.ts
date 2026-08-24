export interface TooltipOptions {
  label: string
  hint?: string
  disabled?: boolean
}

const DELAY = 450
const GAP = 10
const EDGE = 8
let bubble: HTMLDivElement | null = null

function ensureBubble(): HTMLDivElement {
  if (bubble?.isConnected) return bubble
  const node = document.createElement('div')
  node.id = 'pausio-tooltip'
  node.className = 'tooltip'
  node.setAttribute('role', 'tooltip')
  node.hidden = true
  document.body.appendChild(node)
  bubble = node
  return node
}

/** :focus-visible is not implemented by every test DOM; never let a probe crash the app. */
function keyboardFocused(node: HTMLElement): boolean {
  try {
    return node.matches(':focus-visible')
  } catch {
    return false
  }
}

export function tooltip(node: HTMLElement, options: TooltipOptions) {
  let current = options
  let timer: ReturnType<typeof setTimeout> | undefined
  let open = false

  const place = (tip: HTMLDivElement) => {
    const target = node.getBoundingClientRect()
    const size = tip.getBoundingClientRect()
    const above = target.top - size.height - GAP >= EDGE
    const top = above ? target.top - size.height - GAP : target.bottom + GAP
    const left = Math.min(
      Math.max(target.left + target.width / 2 - size.width / 2, EDGE),
      Math.max(EDGE, window.innerWidth - size.width - EDGE)
    )
    tip.dataset.side = above ? 'above' : 'below'
    tip.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`
  }

  const show = () => {
    if (current.disabled || !current.label) return
    const tip = ensureBubble()
    tip.replaceChildren(document.createTextNode(current.label))
    if (current.hint) {
      const key = document.createElement('span')
      key.className = 'tooltip-key'
      key.textContent = current.hint
      tip.appendChild(key)
    }
    tip.hidden = false
    tip.style.transform = 'translate(-9999px, -9999px)' // measure off-screen, then place
    place(tip)
    tip.dataset.open = 'true'
    node.setAttribute('aria-describedby', tip.id)
    open = true
  }

  const hide = () => {
    clearTimeout(timer)
    timer = undefined
    if (!open) return
    open = false
    node.removeAttribute('aria-describedby')
    const tip = ensureBubble()
    delete tip.dataset.open
    tip.hidden = true
  }

  const onEnter = (event: PointerEvent) => {
    if (event.pointerType === 'touch') return
    clearTimeout(timer)
    timer = setTimeout(show, DELAY)
  }
  const onFocus = () => {
    if (keyboardFocused(node)) show()
  }
  const onKey = (event: KeyboardEvent) => {
    if (event.key === 'Escape' && open) {
      event.stopPropagation()
      hide()
    }
  }

  node.addEventListener('pointerenter', onEnter)
  node.addEventListener('pointerleave', hide)
  node.addEventListener('pointerdown', hide)
  node.addEventListener('focusin', onFocus)
  node.addEventListener('focusout', hide)
  node.addEventListener('keydown', onKey)
  window.addEventListener('scroll', hide, true)
  window.addEventListener('resize', hide)

  return {
    update(next: TooltipOptions) {
      current = next
      if (open && (next.disabled || !next.label)) hide()
      else if (open) show()
    },
    destroy() {
      hide()
      node.removeEventListener('pointerenter', onEnter)
      node.removeEventListener('pointerleave', hide)
      node.removeEventListener('pointerdown', hide)
      node.removeEventListener('focusin', onFocus)
      node.removeEventListener('focusout', hide)
      node.removeEventListener('keydown', onKey)
      window.removeEventListener('scroll', hide, true)
      window.removeEventListener('resize', hide)
    },
  }
}
