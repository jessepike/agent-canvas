import { type RefObject, useEffect, useRef } from "react";

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

export function useFocusTrap<T extends HTMLElement>(
  ref: RefObject<T | null>,
  onEscape?: () => void
) {
  // Keep the latest onEscape without making the effect depend on its identity.
  // Dialogs pass inline closures, so depending on `onEscape` would re-run this
  // effect on every render and call first.focus() again — stealing focus
  // mid-typing (cursor jumps out). The effect should only re-run when the trap
  // toggles on/off, so we gate on `enabled` and read the handler from a ref.
  const onEscapeRef = useRef(onEscape);
  onEscapeRef.current = onEscape;
  const enabled = onEscape != null;

  useEffect(() => {
    if (!enabled) {
      return;
    }
    const container = ref.current;
    if (!container) {
      return;
    }
    const activeContainer = container;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    firstFocusable(activeContainer)?.focus();

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onEscapeRef.current?.();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      const focusable = focusableElements(activeContainer);
      if (focusable.length === 0) {
        return;
      }
      const firstElement = focusable[0];
      const lastElement = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === firstElement) {
        event.preventDefault();
        lastElement.focus();
      } else if (!event.shiftKey && document.activeElement === lastElement) {
        event.preventDefault();
        firstElement.focus();
      }
    }

    container.addEventListener("keydown", handleKeyDown);
    return () => {
      container.removeEventListener("keydown", handleKeyDown);
      previous?.focus();
    };
    // Intentionally excludes `onEscape` identity (read via ref) so typing in a
    // dialog does not re-run this effect and re-grab focus.
  }, [enabled, ref]);
}

function firstFocusable(container: HTMLElement): HTMLElement | null {
  return focusableElements(container)[0] ?? null;
}

function focusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (element) => element.offsetParent !== null || element === document.activeElement
  );
}
