// The two rig-level header knobs: the input trim (top left) and the master
// output (top right). They replaced the per-stack volume slider and the
// "In" checkbox that lived in each channel's header.

describe("header volume knobs", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("renders two header knobs in the top bar", () => {
    // Two knobs, in order Input then Master.
    cy.get(".header-knob").should("have.length", 2);
    cy.get(".header-knob").eq(0).contains("Input").should("exist");
    cy.get(".header-knob").eq(1).contains("Master").should("exist");

    // Each knob has a face and a visible dB readout beside it.
    cy.get(".header-knob").eq(0).find(".knob").should("exist");
    cy.get(".header-knob").eq(0).find("span").contains("0.0 dB").should("exist");
    cy.get(".header-knob").eq(1).find(".knob").should("exist");
    cy.get(".header-knob").eq(1).find("span").contains("0.0 dB").should("exist");
  });

  it("places the knobs in the top bar", () => {
    // The first header knob is the first child of the top bar.
    cy.get(".topbar").children().first().should("have.class", "header-knob");
    // The last header knob is the last child of the top-right area.
    cy.get(".top-right").children().last().should("have.class", "header-knob");
  });

  it("has no channel volume slider or input source toggle", () => {
    // The per-stack controls are gone everywhere.
    cy.get(".channel-head input[type=range]").should("not.exist");
    cy.get(".channel-head .mini").should("not.exist");
  });

  it("dragging the Input knob raises its readout by 8.4 dB", () => {
    cy.get(".header-knob").eq(0).trigger("pointerdown", { clientY: 300 });
    cy.get(".header-knob").eq(0).trigger("pointermove", { clientY: 285 });
    cy.get(".header-knob").eq(0).trigger("pointerup");

    // Span 84 dB over 150 px => 15 px = 8.4 dB.
    cy.get(".header-knob").eq(0).find("span").contains("8.4 dB").should("exist");
    // The fake engine's broadcast is what puts the new value back on screen.
    cy.get(".toast").should("not.exist");
  });

  it("dragging the Master knob changes its readout by 7.2 dB", () => {
    cy.get(".header-knob").eq(1).trigger("pointerdown", { clientY: 300 });
    cy.get(".header-knob").eq(1).trigger("pointermove", { clientY: 285 });
    cy.get(".header-knob").eq(1).trigger("pointerup");

    // Span 72 dB over 150 px => 15 px = 7.2 dB.
    cy.get(".header-knob").eq(1).find("span").contains("7.2 dB").should("exist");
    cy.get(".toast").should("not.exist");
  });
});
