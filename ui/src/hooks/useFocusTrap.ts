import { type RefObject, useEffect } from "react";

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

export function useFocusTrap<T extends HTMLElement>(
  ref: RefObject<T | null>,
  onEscape?: () => void
) {
  useEffect(() => {
    if (!onEscape) {
      return;
    }
    const container = ref.current;
    if (!container) {
      return;
    }
    const activeContainer = container;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const first = firstFocusable(activeContainer);
    first?.focus();

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && onEscape) {
        event.preventDefault();
        onEscape();
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

    activeContainer.addEventListener("keydown", handleKeyDown);
    return () => {
      activeContainer.removeEventListener("keydown", handleKeyDown);
      previous?.focus();
    };
  }, [onEscape, ref]);
}

function firstFocusable(container: HTMLElement): HTMLElement | null {
  return focusableElements(container)[0] ?? null;
}

function focusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (element) => element.offsetParent !== null || element === document.activeElement
  );
}
