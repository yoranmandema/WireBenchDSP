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
