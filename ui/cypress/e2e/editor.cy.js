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

  it("renders the knob arc and indicator from the current value", () => {
    const circles = () => cy.get(".knob-unit").eq(0).find(".knob-arc circle");
    let initialDash;
    let initialTransform;
    circles().eq(0).should("have.attr", "stroke-dasharray", "75 100");
    circles().eq(0).should("have.attr", "stroke-width", "4");
    circles().eq(1).invoke("attr", "stroke-dasharray").then((value) => { initialDash = value; });
    cy.get(".knob-indicator").invoke("attr", "style").then((value) => { initialTransform = value; });

    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerdown", { pointerId: 1, button: 0, clientY: 300 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointermove", { pointerId: 1, clientY: 100 });
    cy.get(".knob-unit").eq(0).find(".knob")
      .trigger("pointerup", { pointerId: 1 });

    cy.get(".knob-unit").eq(0).find(".knob-value").should("not.have.text", "5.00");
    circles().eq(1).should(($circle) => {
      expect($circle.attr("stroke-dasharray")).not.to.equal(initialDash);
    });
    cy.get(".knob-indicator").should(($indicator) => {
      expect($indicator.attr("style")).not.to.equal(initialTransform);
    });
  });

  it("keeps the indicator pivot at the visual center", () => {
    cy.get(".knob").eq(0).should(($knob) => {
      const knob = $knob[0];
      const indicator = knob.querySelector(".knob-indicator");
      const style = getComputedStyle(indicator);
      const [originX, originY] = style.transformOrigin.split(" ").map(parseFloat);

      expect(style.translate).to.equal("none");
      expect(indicator.offsetLeft + originX - indicator.offsetWidth / 2)
        .to.be.closeTo(knob.clientWidth / 2, 0.5);
      expect(indicator.offsetTop + originY).to.be.closeTo(knob.clientHeight / 2, 0.5);
    });
  });

  it("updates visual feedback for a knob on a second block type", () => {
    cy.selectBlock(0, 1); // Delay, with a 20–1000 ms range
    cy.get(".editor-name").should("contain", "Delay");

    const knob = () => cy.get(".knob-unit").eq(0);
    let initialValue;
    let initialDash;
    let initialTransform;
    knob().find(".knob-value").invoke("text").then((value) => { initialValue = value; });
    knob().find(".knob-arc circle").eq(1).invoke("attr", "stroke-dasharray")
      .then((value) => { initialDash = value; });
    knob().find(".knob-indicator").invoke("attr", "style")
      .then((value) => { initialTransform = value; });

    knob().find(".knob").trigger("pointerdown", { pointerId: 1, button: 0, clientY: 300 });
    knob().find(".knob").trigger("pointermove", { pointerId: 1, clientY: 250 });
    knob().find(".knob").trigger("pointerup", { pointerId: 1 });

    knob().find(".knob-value").should(($value) => {
      expect($value.text()).not.to.equal(initialValue);
    });
    knob().find(".knob-arc circle").eq(1).should(($circle) => {
      expect($circle.attr("stroke-dasharray")).not.to.equal(initialDash);
    });
    knob().find(".knob-indicator").should(($indicator) => {
      expect($indicator.attr("style")).not.to.equal(initialTransform);
    });
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
