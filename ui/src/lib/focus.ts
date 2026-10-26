import { editor } from "./editor.svelte";

// Keys go to the score only while it has focus, so controls around it hand focus back after
// use. Open menus and dialogs keep it.
function returnFocusToScore() {
  requestAnimationFrame(() => {
    if (document.querySelector(".menu-panel, dialog[open]")) return;
    editor.focusNoteEditor();
  });
}

export function returnFocusAfterClick(event: MouseEvent) {
  if ((event.target as Element).closest("button")) returnFocusToScore();
}

// A text or number field keeps focus while it is being edited.
export function returnFocusAfterChoice(event: Event) {
  if (event.target instanceof HTMLSelectElement) returnFocusToScore();
}
