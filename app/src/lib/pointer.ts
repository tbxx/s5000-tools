/** Keeps pointer events coming to the element while the button is held, so
 *  a release outside it is still seen. Capture can fail (a click with no live
 *  pointer, e.g. from accessibility tools), and must never block the action. */
export function capture(e: PointerEvent) {
  try {
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  } catch {
    // Without capture the action still works; a release outside is missed.
  }
}
