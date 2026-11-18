// Shared Cypress commands for the E2E suite.
//
// `cy.boot()` is the entry point of nearly every test: it loads the app against
// the fake engine and waits until the rig has rendered. The fake engine resets
// to its seeded state on every connection, so each test starts from the same
// rig.

Cypress.Commands.add("boot", () => {
  cy.visit("/", {
    onBeforeLoad(win) {
      // The preset browser confirms deletions. Accept them deterministically
      // instead of depending on Cypress's dialog handling.
      win.confirm = () => true;
    },
  });
  cy.get(".topbar", { timeout: 15000 }).should("exist");
});

Cypress.Commands.add("openPresetBrowser", () => {
  cy.get(".presets-open").click();
  cy.get(".modal").should("be.visible");
});

/** Click a block card in a channel row and wait for the editor to show it. */
Cypress.Commands.add("selectBlock", (channelIndex, blockIndex) => {
  cy.get(".channel")
    .eq(channelIndex)
    .find(".block-card")
    .eq(blockIndex)
    .click();
  cy.get(".editor-name").should("exist");
});

/** Click the channel "+" to open the add-block picker, filter by `type`, and
 *  confirm the matching block. Closes the modal on selection. */
Cypress.Commands.add("addBlock", (type, name) => {
  // Wait for the engine state to land: the top bar shows the preset name only
  // once `ready` is true (i.e. the snapshot has been folded into
  // `state.value`). The "Connecting…" screen would otherwise mask the picker.
  cy.get(".preset-name").should("not.contain", "Untitled");
  cy.get(`[data-add="ch1"]`).should("be.visible");
  cy.get(`[data-add="ch1"]`).invoke("click");
  // Give the signal a tick so the picker remounts.
  cy.wait(500);
  cy.get(".modal").should("be.visible");
  cy.get(".picker-search").type(type);
  cy.get(".picker-body").find(`button[data-block-type="${type}"]`).should("be.visible");
  cy.get(".picker-body").find(`button[data-block-type="${type}"]`).click();
  cy.get(".modal").should("not.be.visible");
});

/**
 * Dispatch a pointer event with an explicit `pointerType`.
 *
 * Cypress's `.trigger()` builds the event without one, and the app's gesture
 * code treats an absent `pointerType` as a mouse — so a touch test has to say
 * `pointerType: "touch"` or the long press never starts.
 */
Cypress.Commands.add("pointer", { prevSubject: "element" }, (el, type, opts = {}) => {
  const win = el.ownerDocument.defaultView;
  const e = new win.PointerEvent(type, {
    bubbles: true,
    cancelable: true,
    pointerId: 1,
    pointerType: "mouse",
    ...opts,
  });
  el.dispatchEvent(e);
});
