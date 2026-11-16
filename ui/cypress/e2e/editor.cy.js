// The block editor: identity header, knobs, choice controls, reset, bypass.

describe("block editor", () => {
  beforeEach(() => {
    cy.boot();
    cy.selectBlock(0, 0); // Amp in Channel 1
  });

  it("shows the selected block's identity and controls", () => {
    cy.get(".editor-kind").should("contain", "Selected block");
    cy.get(".editor-name").should("contain", "Amp");
    cy.get(".editor-type").should("contain", "amp.basic");
    cy.get(".knob-unit").should("have.length", 5);
    cy.get(".segmented").should("have.length", 1);
    cy.get(".knob-unit").eq(0).find(".knob-name").should("have.text", "Gain");
    cy.get(".knob-unit").eq(0).find(".knob-value").should("have.text", "5.00");
  });

  it("changes a choice parameter from the segmented control", () => {
    cy.get(".seg").contains("Lead").click();
    cy.get(".seg.on").should("contain", "Lead");

    cy.get(".seg").contains("Crunch").click();
    cy.get(".seg.on").should("contain", "Crunch");
  });

  it("drags a knob to change a number parameter", () => {
    cy.get(".knob-unit").eq(0).find(".knob-value").should("have.text", "5.00");

    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerdown", { pointerId: 1, button: 0, clientY: 300 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointermove", { pointerId: 1, clientY: 100 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerup", { pointerId: 1 });

    cy.get(".knob-unit").eq(0).find(".knob-value").should("not.have.text", "5.00");
  });

  it("resets a knob to its default on double-click", () => {
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerdown", { pointerId: 1, button: 0, clientY: 300 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointermove", { pointerId: 1, clientY: 100 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerup", { pointerId: 1 });
    cy.get(".knob-unit").eq(0).find(".knob-value").should("not.have.text", "5.00");

    cy.get(".knob-unit").eq(0).find(".knob").dblclick();
    cy.get(".knob-unit").eq(0).find(".knob-value").should("have.text", "5.00");
  });

  it("resets every parameter from the editor header", () => {
    cy.get(".seg").contains("Lead").click();
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerdown", { pointerId: 1, button: 0, clientY: 300 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointermove", { pointerId: 1, clientY: 100 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerup", { pointerId: 1 });

    cy.get(".side-btn").contains("Reset").click();

    cy.get(".seg.on").should("contain", "Crunch");
    cy.get(".knob-unit").eq(0).find(".knob-value").should("have.text", "5.00");
  });

  it("toggles bypass from the editor header", () => {
    cy.get(".side-btn").contains("Bypass").click();

    cy.get(".editor-bypass").should("contain", "bypassed");
    cy.get(".side-btn").contains("Bypassed").should("exist");
    cy.get(".channel").eq(0).find(".block-card").eq(0).should("have.class", "bypass");
  });

  it("renders a choice with more than five options as a select", () => {
    cy.selectBlock(1, 1); // Voices in Channel 2

    cy.get(".editor-name").should("contain", "Voices");
    cy.get(".select").should("have.length", 1);
    cy.get(".select option").should("have.length", 6);

    cy.get(".select").select("4");
    cy.get(".select").should("have.value", "4");
  });
});
