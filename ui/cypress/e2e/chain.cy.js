// The signal chain: channel rows, block cards, selection, bypass, remove,
// volume, sources, and the two drag interactions.

describe("signal chain", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("renders four channels with their names", () => {
    cy.get(".channel").should("have.length", 4);
    cy.get(".channel-name")
      .then(($n) => [...$n].map((el) => el.textContent.trim()))
      .should("deep.equal", ["Channel 1", "Channel 2", "Channel 3", "Channel 4"]);
  });

  it("shows an empty-channel hint", () => {
    cy.get(".channel").eq(2).find(".strip-empty").should("contain", "Empty channel");
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
  });

  it("selects a block and shows it in the editor", () => {
    cy.selectBlock(0, 0);
    cy.get(".block-card.selected").should("have.length", 1);
    cy.get(".editor-name").should("contain", "Amp");
    cy.get(".editor-type").should("contain", "amp.basic");
  });

  it("toggles bypass from the card", () => {
    cy.selectBlock(0, 0);
    cy.get(".channel").eq(0).find(".block-card").eq(0).find(".bypass").click();

    cy.get(".channel").eq(0).find(".block-card").eq(0).should("have.class", "bypass");
    cy.get(".editor-bypass").should("contain", "bypassed");
    cy.get(".side-btn").contains("Bypassed").should("exist");
  });

  it("removes a block", () => {
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);

    cy.get(".channel").eq(0).find(".block-card").eq(1).find(".remove").click();

    cy.get(".channel").eq(0).find(".block-card").should("have.length", 2);
    cy.get(".channel").eq(0).find(".block-card").should("not.contain", "Delay");
  });

  it("shows a toast when the engine rejects a removal", () => {
    // The seeded `locked` block always refuses `removeBlock`.
    cy.get(".channel").eq(0).find(".block-card").eq(2).find(".remove").click();

    cy.get(".toast").should("contain", "no such block");
    cy.get(".toast").click();
    cy.get(".toast").should("not.exist");
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
  });

  it("changes the channel volume", () => {
    cy.get(".channel").eq(0).find('input[type="range"]')
      .invoke("val", "-12")
      .trigger("input");

    cy.get(".channel").eq(0).find('input[type="range"]').should("have.value", "-12");
    cy.get(".channel").eq(0).find('input[type="range"]').should("have.attr", "title", "Volume -12.0 dB");
  });

  it("toggles the channel input source", () => {
    const box = () => cy.get(".channel").eq(0).find(".mini input[type='checkbox']");
    box().should("be.checked");
    box().uncheck();
    box().should("not.be.checked");
    box().check();
    box().should("be.checked");
  });

  it("reorders a block by dragging it to another slot", () => {
    const cards = () => cy.get(".channel").eq(0).find(".block-card");
    cards().eq(0).find(".block-name").should("have.text", "Amp");

    cy.get(".channel").eq(0).find(".strip").then(($strip) => {
      const r = $strip[0].getBoundingClientRect();
      const y = Math.round(r.top + r.height / 2);
      const startX = Math.round(r.left + 40);
      const endX = Math.round(r.right - 5);

      cards().eq(0).trigger("pointerdown", { pointerId: 1, button: 0, clientX: startX, clientY: y });
      cards().eq(0).trigger("pointermove", { pointerId: 1, clientX: endX, clientY: y });
      cards().eq(0).trigger("pointerup", { pointerId: 1 });
    });

    cards().eq(0).find(".block-name").should("have.text", "Delay");
    cards().eq(2).find(".block-name").should("have.text", "Amp");
  });

  it("inserts a block dragged from the rack into a channel", () => {
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);

    cy.window().then((win) => {
      const dt = new win.DataTransfer();
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        row.dispatchEvent(new win.DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: dt }));
        cy.get(".channel").eq(2).then(($ch) => {
          const ch = $ch[0];
          ch.dispatchEvent(new win.DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: dt }));
          ch.dispatchEvent(new win.DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
          ch.dispatchEvent(new win.DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt }));
          row.dispatchEvent(new win.DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: dt }));
        });
      });
    });

    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".block-card").should("contain", "Reverb");
  });
});
