// Keyboard access: block cards are focusable and selectable with Enter/Space.
//
// Cypress's `.trigger("keydown")` dispatches at the element's centre, which
// lands on the inner `.block-name` span; the card's own handler ignores events
// whose target is not the card. A real keypress on a focused card has the card
// as its target, so dispatch the keydown on the card itself.

function pressKey(selector, key, index = 0) {
  cy.get(selector).eq(index).then(($el) => {
    const el = $el[0];
    const win = el.ownerDocument.defaultView;
    el.dispatchEvent(new win.KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  });
}

describe("keyboard selection", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("selects a block with Enter and draws the selection ring", () => {
    cy.get(".channel").eq(0).find(".block-card").eq(0).focus();
    pressKey(".channel .block-card", "Enter", 0);

    cy.get(".block-card.selected").should("have.length", 1);
    cy.get(".editor-name").should("contain", "Amp");
    cy.get(".selection-ring").should("exist");
  });

  it("selects a block with Space", () => {
    cy.get(".channel").eq(0).find(".block-card").eq(1).focus();
    pressKey(".channel .block-card", " ", 1);

    cy.get(".editor-name").should("contain", "Delay");
  });
});
